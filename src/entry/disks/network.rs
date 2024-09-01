/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

use std::{
    cell::UnsafeCell,
    cmp::min,
    fmt::Debug,
    future::Future,
    marker::PhantomPinned,
    pin::Pin,
    sync::{Mutex, OnceLock},
    task::{ready, Context, Poll},
    time::Duration,
};

use bytes::Bytes;
use futures::io::AsyncRead;
use pin_project::pin_project;
use reqwest::{header::HeaderMap, Client, Method, Request, Response, Url, Version};
use serde::{Deserialize, Serialize};

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

#[pin_project(!Unpin)]
/// Wraps a [`reqwest::Response`] to provide [`AsyncRead`].
pub struct ReqwestRead {
    response: Response,
    chunk: Option<Pin<Box<dyn Future<Output = reqwest::Result<Option<Bytes>>> + Send + Sync>>>,
    leftover_slice: Option<Vec<u8>>,
}

// This is safe, but UnsafeCell disables by default.
unsafe impl Sync for ReqwestRead {}
unsafe impl Send for ReqwestRead {}

impl AsyncRead for ReqwestRead {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<std::io::Result<usize>> {
        let this = self.as_mut().project();

        // Check for leftovers and fill those in first.
        if let Some(leftover_slice) = &this.leftover_slice {
            let buf_len = buf.len();
            let num_bytes = min(leftover_slice.len(), buf_len);

            if leftover_slice.len() > buf_len {
                // Write partial and advance
                buf.copy_from_slice(&leftover_slice[..buf_len]);
                *this.leftover_slice = Some(leftover_slice[buf_len..].to_vec());
            } else {
                // Write full and clear
                buf[..leftover_slice.len()].copy_from_slice(leftover_slice);
                *this.leftover_slice = None;
            };

            return Poll::Ready(Ok(num_bytes));
        }

        loop {
            if let Some(chunk) = this.chunk.as_mut() {
                let chunk_res = ready!(chunk.as_mut().poll(cx));

                let processed_poll = match chunk_res {
                    Err(x) => Poll::Ready(Err(reqwest_err_to_io(x))),
                    Ok(None) => Poll::Ready(Ok(0)),
                    Ok(Some(x)) => {
                        // Clear out this chunk as fully polled
                        *this.chunk = None;

                        // Get current chunk metadata
                        let buf_len = buf.len();
                        let num_bytes = min(x.len(), buf_len);

                        // Write in current chunk data.
                        if x.len() > buf_len {
                            // Write partial and advance
                            buf.copy_from_slice(&x[..buf_len]);
                            *this.leftover_slice = Some(x[buf_len..].to_vec());
                        } else {
                            // Write full, no need to create leftovers
                            buf[..x.len()].copy_from_slice(&x);
                        };
                        Poll::Ready(Ok(num_bytes))
                    }
                };

                return processed_poll;
            } else {
                // This is the only location response is borrowed mutably, it
                // always clears a prior mutable borrow before returning here,
                // and `!Unpin` ensures the pointer remains valid.
                let response_ptr: *mut _ = this.response;
                let response = unsafe { &mut *response_ptr };

                *this.chunk = Some(Box::pin(response.chunk()));
            }
        }
    }
}

#[pin_project(!Unpin)]
pub struct ReqwestReadBuilder {
    client: Client,
    request: SerialRequest,
    fut: Option<Pin<Box<dyn Future<Output = Result<Response, reqwest::Error>>>>>,
}

impl Debug for ReqwestReadBuilder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let fut_ptr: *const _;
        let fut: &dyn Debug = match &self.fut {
            Some(x) => {
                fut_ptr = &*x.as_ref();
                &fut_ptr
            }
            None => &None::<()>,
        };

        f.debug_struct("ReqwestReadBuilder")
            .field("fut", &fut)
            .finish()
    }
}

impl Future for ReqwestReadBuilder {
    type Output = std::io::Result<ReqwestRead>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut this = self.as_mut().project();

        if let Some(mut fut) = this.fut.as_mut() {
            let res = ready!(fut.as_mut().poll(cx));
            match res {
                Ok(response) => Poll::Ready(Ok(ReqwestRead {
                    response,
                    chunk: None,
                    leftover_slice: None,
                })),
                Err(e) => {
                    let e = reqwest_err_to_io(e);
                    Poll::Ready(Err(e))
                }
            }
        } else {
            let client_ptr: *mut _ = this.client;
            let client = unsafe { &mut *client_ptr };

            let request_ptr: *const _ = this.request;
            let request = unsafe { &*request_ptr };

            *this.fut = Some(Box::pin(client.execute(request.get())));
            self.poll(cx)
        }
    }
}

impl ReqwestReadBuilder {
    pub fn new(request: SerialRequest, client: Client) -> Self {
        Self {
            client,
            request,
            fut: None,
        }
    }
}

impl AsyncReadDisk for Network {
    type ReadDisk = ReqwestRead;
    type ReadFut = ReqwestReadBuilder;

    fn async_read_disk(&self) -> Self::ReadFut {
        let client = {
            let mut client_handle = self.client.lock().unwrap();
            client_handle.get_or_insert(default_client()).clone()
        };

        ReqwestReadBuilder::new(self.request.clone(), client)
    }
}

#[cfg(test)]
#[cfg(not(miri))] // Miri can't do TLS calls
mod tests {
    use super::*;

    #[tokio::test]
    #[cfg(feature = "async_csv")]
    async fn remote_read() {
        use std::{pin::pin, str::FromStr};

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

        let disk = pin!(disk.async_read_disk()).await.unwrap();
        let from_remote: Vec<IouZipcodes> = coder.decode(pin!(disk)).await.unwrap();

        assert_eq!(from_remote[0], *FIRST_ENTRY);
        assert_eq!(from_remote[from_remote.len() - 1], *LAST_ENTRY);
    }
}
