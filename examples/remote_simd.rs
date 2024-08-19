#![cfg(all(feature = "simd_json", feature = "async", feature = "network"))]

/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

use std::{future::ready, str::FromStr};

use backed_data::{
    entry::{
        adapters::{DecodeBg, SyncCoderAsyncDisk},
        disks::{AsyncReadDisk, Network},
        formats::{AsyncDecoder, SimdJsonCoder},
    },
    utils::blocking::BlockingFn,
};
use reqwest::{Method, Request};
use serde::{Deserialize, Serialize};
use url::Url;

const COURSE_URL: &str =
    "https://www.lib.ncsu.edu/api/course-guides/everything.json?curriculum=ece";

#[tokio::main]
async fn main() {
    let disk = Network::new(
        Request::new(Method::GET, Url::from_str(COURSE_URL).unwrap()),
        None,
    );

    // Create an adapter to run decoding in tokio's blocking threadpool, on top
    // of an asynchronous disk. Specify unit types for writes and write future
    // returns, as those are not used.
    let coder = SyncCoderAsyncDisk::<_, Network, _, _, _, ()>::new(
        SimdJsonCoder::default(),
        |x: DecodeBg<_, _>| ready(x.call()),
        (),
    );

    // Decode asynchronously using the asynchronous disk and a blocking thread.
    let decoded: Vec<NcsuCourseListings> = coder
        .decode(disk.async_read_disk().await.unwrap())
        .await
        .unwrap();

    // The data is now loaded locally and available.
    let ece_109 = decoded[0]
        .children
        .iter()
        .find(|child| child.course == "109")
        .unwrap();
    println!("ECE 109: {:#?}", ece_109);
}

// ---------- Code beyond this point is unrelated to the processing pipeline ---------- //

// All following code is generated with `transform`
// (https://github.com/ritz078/transform), and slightly post-processed.
#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NcsuCourseListings {
    pub does_course_exist: bool,
    pub inherited_content: bool,
    pub curriculum: String,
    pub course_code: Option<String>,
    pub curriculum_name: String,
    pub section: Option<String>,
    pub title: String,
    pub description: String,
    pub subject_specialist: SubjectSpecialist,
    pub tid: String,
    pub last_updated: String,
    pub instructors: Vec<String>,
    pub recommended_content: Vec<RecommendedContent>,
    pub children: Vec<Children>,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SubjectSpecialist {
    pub name: String,
    pub unity: String,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecommendedContent {
    pub name: String,
    pub description: String,
    pub links: Vec<Link>,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Link {
    pub link_label: String,
    pub url: String,
    pub annotation: String,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Children {
    pub course: String,
    #[serde(default)]
    pub instructors: Vec<String>,
    pub librarian: Librarian,
    pub course_title: Option<String>,
    pub path: String,
    pub json: String,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Librarian {
    pub name: String,
    pub unity: String,
}
