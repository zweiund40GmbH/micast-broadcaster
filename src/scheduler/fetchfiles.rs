

use std::io::{copy, Cursor};

use reqwest;
use reqwest_middleware::{ClientBuilder, ClientWithMiddleware};
use reqwest_retry::{RetryTransientMiddleware, policies::ExponentialBackoff};

use std::fs::File;
use std::path::Path;
use async_std::task;
use log::info;
use anyhow::{Error, anyhow};
use chrono::prelude::*;
use std::env;

use std::sync::mpsc::Sender;

use super::parser;

use futures::stream::{self, StreamExt, TryStreamExt};


async fn download_file ( file: parser::File, counter: u32) -> Result<parser::File, Error> {

    let dir = env::temp_dir();
    let local: DateTime<Local> = Local::now();

    let filename = format!("{}-{}_{}.mp3", file.id, counter, local.timestamp_millis());
    let local_path = Path::new(&dir).join(filename);

    let new_file = parser::File {
        id: file.id,
        uri: file.uri.clone(),
        local: Some(local_path.clone().display().to_string())
    };

    info!("want to download {} -> {}", file.uri.clone(), local_path.clone().display());

    let retry_policy = ExponentialBackoff::builder().build_with_max_retries(10);
    let client = ClientBuilder::new(reqwest::Client::new())
        .with(RetryTransientMiddleware::new_with_policy(retry_policy))
        .build();

    match client.get(file.uri.clone()).send().await {
        Ok(res) => {
            match res.bytes().await {
                Ok(b) => {
                    let mut dest = File::create(local_path)?;
                    let mut output = Cursor::new(b);
                    match copy(&mut output, &mut dest) {
                        Ok(_) => {
                            Ok(new_file)
                        },
                        Err(e) => {
                            Err(anyhow!("io error: {}", e))
                        }
                    }
                }, 
                Err(e) => {
                    
                    Err(anyhow!("request error: {}", e))
                }
            }
        },
        Err(e) => {
            Err(anyhow!("request error: {}", e))
        }
    }


}

pub fn download_files(files: Vec<parser::File>, ready_sender: Sender<Vec<parser::File>>) {

    info!("spawn async task for file downloading");


    

    std::thread::spawn( move || {

        let res = task::block_on(async {

            let fetches = stream::iter(files).map(|file| {
                download_file(file, 0)    
            }).map(Ok)
            .try_buffer_unordered(6)
            .map_err(|e| {
                info!("got an error while downloading: {:?}", e)
            }).try_collect::<Vec<parser::File>>();
    
            info!("wait...");
            fetches.await

        });

        

        info!("all files downloaded: {:#?}", res);
        

        let _ = ready_sender.send(res.unwrap());
    });

    


    
}