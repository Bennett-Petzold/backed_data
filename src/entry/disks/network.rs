use std::{
    cell::UnsafeCell,
    future::Future,
    pin::Pin,
    sync::{Mutex, OnceLock},
    task::{Context, Poll},
    time::Duration,
};

use bytes::Bytes;
use reqwest::{header::HeaderMap, Client, Method, Request, Response, Url, Version};
use secrets::traits::AsContiguousBytes;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncRead, ReadBuf};

#[cfg(feature = "async")]
use super::AsyncReadDisk;

static NETWORK_CLIENT: OnceLock<Client> = OnceLock::new();

/// Clones out a new client from an internal static pool.
pub fn default_client() -> Client {
    NETWORK_CLIENT.get_or_init(Client::new).clone()
}

/// Wraps [`reqwest::Request`] so it can be be (de)serialized.
#[derive(Debug, Serialize, Deserialize)]
struct SerialRequest {
    //#[serde(serialize_with = "to_method_enum")]
    //#[serde(deserialize_with = "from_method_enum")]
    #[serde(with = "http_serde::method")]
    method: Method,
    url: Url,
    #[serde(with = "http_serde::header_map")]
    headers: HeaderMap,
    timeout: Option<Duration>,
    #[serde(with = "http_serde::version")]
    version: Version,
}

impl Clone for SerialRequest {
    fn clone(&self) -> Self {
        Self {
            method: self.method.clone(),
            url: self.url.clone(),
            headers: self.headers.clone(),
            timeout: self.timeout,
            version: self.version,
        }
    }
}

impl SerialRequest {
    fn get(&self) -> Request {
        let this = self.clone();
        let mut req = Request::new(this.method, this.url);
        *req.headers_mut() = this.headers;
        *req.timeout_mut() = this.timeout;
        *req.version_mut() = this.version;
        req
    }
}

impl From<Request> for SerialRequest {
    fn from(value: Request) -> Self {
        Self {
            method: value.method().clone(),
            url: value.url().clone(),
            headers: value.headers().clone(),
            timeout: value.timeout().copied(),
            version: value.version(),
        }
    }
}

/// Represents some inbound network request.
///
/// Holds a static request, which will be run to provide disk read. The client
/// is meant to be freely swapped to share pools as appropriate. If no client
/// is provided (and when deserialized, no client is provided until set), the
/// internal static pool [`default_client`] will be used.
#[derive(Debug, Serialize, Deserialize)]
pub struct Network {
    /// File location.
    request: SerialRequest,
    #[serde(skip)]
    pub client: Mutex<Option<Client>>,
}

impl Clone for Network {
    fn clone(&self) -> Self {
        Self {
            request: self.request.clone(),
            client: Mutex::new(self.client.lock().unwrap().clone()),
        }
    }
}

impl Network {
    /// Constructs a new [`Network`] to represent some inbound network request.
    ///
    /// * `request`: The inbound request to wrap. Must return a byte stream.
    /// * `client`: If no client is given, a default global one is used.
    pub fn new(request: Request, client: Option<Client>) -> Self {
        Self {
            request: request.into(),
            client: client.into(),
        }
    }
}

fn reqwest_err_to_io(e: reqwest::Error) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::Other, e)
}

/// Wraps a [`reqwest::Response`] to provide [`AsyncRead`].
pub struct ReqwestRead {
    response: Pin<Box<UnsafeCell<Response>>>,
    chunk: Pin<Box<dyn Future<Output = reqwest::Result<Option<Bytes>>>>>,
    leftover_slice: Option<Vec<u8>>,
}

// This is safe, but UnsafeCell disables by default.
unsafe impl Sync for ReqwestRead {}
unsafe impl Send for ReqwestRead {}

impl AsyncRead for ReqwestRead {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        // Check for leftovers and fill those in first.
        {
            let self_mut = self.as_mut().get_mut();
            let leftover_slice = Pin::new(&mut self_mut.leftover_slice);
            if let Some(leftover_slice) = leftover_slice.get_mut() {
                let remaining = buf.remaining();

                if leftover_slice.len() > remaining {
                    // Write partial and advance
                    buf.put_slice(&leftover_slice[..remaining]);
                    self_mut.leftover_slice = Some(leftover_slice[remaining..].to_vec());
                } else {
                    // Write full and clear
                    buf.put_slice(leftover_slice);
                    self_mut.leftover_slice = None;
                };

                return Poll::Ready(Ok(()));
            }
        }

        // Needs this form to satisfy borrow checker.
        let chunk_res = {
            let mut chunk = Pin::new(&mut self.as_mut().get_mut().chunk);
            chunk.as_mut().poll(cx)
        };

        match chunk_res {
            Poll::Pending => Poll::Pending,
            Poll::Ready(Err(x)) => Poll::Ready(Err(reqwest_err_to_io(x))),
            Poll::Ready(Ok(None)) => Poll::Ready(Ok(())),
            Poll::Ready(Ok(Some(x))) => {
                // Just a little laundry to grab out the next future.
                let this: *mut _ = self.as_mut().get_mut();
                let new_chunk = Box::pin(unsafe { &mut *this }.response.get_mut().chunk());

                // Update to next chunk.
                let self_mut = self.as_mut().get_mut();
                let chunk = Pin::new(&mut self_mut.chunk);
                *chunk.get_mut() = new_chunk;

                // Write in current chunk data.
                let x = x.as_bytes();
                let remaining = buf.remaining();
                if x.len() > remaining {
                    // Write partial and advance
                    buf.put_slice(&x[..remaining]);
                    self_mut.leftover_slice = Some(x[remaining..].to_vec());
                } else {
                    // Write full, no need to create leftovers
                    buf.put_slice(x);
                };
                Poll::Ready(Ok(()))
            }
        }
    }
}

#[cfg(feature = "async")]
impl AsyncReadDisk for Network {
    //type ReadDisk = StreamReader<impl Stream<Item = Result<u8, u8>, u8>>;
    type ReadDisk = ReqwestRead;

    async fn async_read_disk(&self) -> std::io::Result<Self::ReadDisk> {
        let client = {
            let mut client_handle = self.client.lock().unwrap();
            client_handle.get_or_insert(default_client()).clone()
        };

        client
            .execute(self.request.get())
            .await
            .map(|response| {
                // Need a little unsafe hackery to pull in the first chunk
                let response = Box::pin(UnsafeCell::new(response));
                let chunk = Box::pin(unsafe { &mut *response.get() }.chunk());
                ReqwestRead {
                    response,
                    chunk,
                    leftover_slice: None,
                }
            })
            .map_err(reqwest_err_to_io)
    }
}

#[cfg(test)]
#[cfg(not(miri))] // Miri can't do TLS calls
mod tests {
    use super::*;

    #[tokio::test]
    #[cfg(feature = "async_csv")]
    async fn remote_read() {
        use std::str::FromStr;

        use crate::{
            entry::formats::{AsyncCsvCoder, AsyncDecoder},
            test_utils::csv_data::{IouZipcodes, FIRST_ENTRY, LAST_ENTRY},
        };

        let disk = Network::new(
            Request::new(
                Method::GET,
                Url::from_str("https://data.openei.org/files/5650/iou_zipcodes_2020.csv").unwrap(),
            ),
            None,
        );
        let coder = AsyncCsvCoder::default();

        let from_remote: Vec<IouZipcodes> = coder
            .decode(&mut disk.async_read_disk().await.unwrap())
            .await
            .unwrap();

        assert_eq!(from_remote[0], *FIRST_ENTRY);
        assert_eq!(from_remote[from_remote.len() - 1], *LAST_ENTRY);
    }
}
