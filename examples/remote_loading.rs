use std::{str::FromStr, time::SystemTime};

use backed_data::{
    entry::{disks::Network, formats::AsyncCsvCoder, BackedEntryAsync},
    test_utils::csv_data::IouZipcodes,
};
use reqwest::{Method, Request, Url};

const URL: &str = "https://data.openei.org/files/5650/iou_zipcodes_2020.csv";

#[tokio::main]
async fn main() {
    // Construct the backed entry for electric utility data.
    // Note that AsyncCsvCoder needs more type hints than other formats.
    let mut backed_entry = BackedEntryAsync::new(
        Network::new(Request::new(Method::GET, Url::from_str(URL).unwrap()), None),
        AsyncCsvCoder::<Box<[IouZipcodes]>, _>::default(),
    );

    let prior_to_first_load = SystemTime::now();

    // Load in the dataset and check length.
    let num_rows = backed_entry.a_load().await.unwrap().len();
    println!(
        "Time to load from network: {:#?}\n",
        SystemTime::now()
            .duration_since(prior_to_first_load)
            .unwrap()
    );
    println!("Number of rows: {}", num_rows);
    println!(
        "Dataset size: {:.2} MiB",
        ((num_rows * size_of::<IouZipcodes>()) as f64) / ((1024 * 1024) as f64)
    );

    let prior_to_second_load = SystemTime::now();

    // Get an entry, reusing the version loaded in memory.
    let entry = backed_entry.a_load().await.unwrap().get(30_000).unwrap();
    println!("Line 30,000: {:#?}", entry);
    println!(
        "Time to load from memory: {:#?}\n",
        SystemTime::now()
            .duration_since(prior_to_second_load)
            .unwrap()
    );

    // Drop the value from memory.
    backed_entry.unload();

    let prior_to_third_load = SystemTime::now();

    let entry = backed_entry.a_load().await.unwrap().get(10_000).unwrap();
    println!(
        "Time to load from network (again): {:#?}",
        SystemTime::now()
            .duration_since(prior_to_third_load)
            .unwrap()
    );
    println!("Line 10,000: {:#?}", entry);
}
