

use std::io::BufWriter;
use anyhow::{*, Result};
use futures::StreamExt;
use async_std::task;

use rodio::{Decoder, OutputStream, source::Source, Sink};
use symphonia::core::io::{ReadOnlySource, MediaSource, MediaSourceStream};

use std::sync::mpsc::{channel, Receiver};
use bytes::{Buf, BufMut, BytesMut};
use std::sync::Arc;

pub struct Broadcaster {
}

impl Broadcaster {

    pub fn new() -> Result<Broadcaster> {
       Ok(Broadcaster {}) 
    }

    pub fn play(&self, uri: String) -> Result<()> {

        let buf: BytesMut = BytesMut::new();
        let mut cloned_buf = buf.clone();
        let c = buf.reader();

        std::thread::spawn( move || {
            let res = task::block_on(async {
                let client = reqwest::Client::new();
                let head_response = client.head(uri.clone()).send().await?;

                println!("got head_response: {:#?}", head_response);

                let req = client.get(uri).send().await?;

                let mut stream = req.bytes_stream();

                // some preroll
                while let Some(item) = stream.next().await {
                    let by = item.unwrap();
                        cloned_buf.put(by);
                        //println!("got a chunk: {:?}", item?);
                }

                Ok(())
            });
        });


        // hier kommt der receiver
        let (_stream, stream_handle) = rodio::OutputStream::try_default().unwrap();
        let sink = Sink::try_new(&stream_handle).unwrap();

        //let source = Box::new(ReadOnlySource::new(c)) as Box<dyn MediaSource>;
        //let mss = MediaSourceStream::new(source, Default::default());

        let sdec = Decoder::new_mp3(c)?;

        sink.append(sdec);
        sink.play();
        sink.sleep_until_end();


        Ok(())
    }
}

