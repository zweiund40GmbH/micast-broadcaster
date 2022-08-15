/// Encryption Callbacks for RTP RTCP 
/// 
/// 
use gst::prelude::*;
use gst::glib;
use anyhow::{Result};
use log::debug;
use crate::helpers::make_element;


#[derive(Clone)]
enum DeOrEncoder {
    Encoder,
    Decoder
}

impl Into<String> for DeOrEncoder {
    fn into(self) -> String {
        match self {
            DeOrEncoder::Encoder => String::from("encoder"),
            DeOrEncoder::Decoder => String::from("decoder"),
        }
    }
}

impl DeOrEncoder {
    fn to_string(self) -> String {
        self.into()
    }
}

#[derive(Clone, PartialEq)]
enum RTPorRTCP {
    Rtp,
    Rtcp
}

impl Into<String> for RTPorRTCP {
    fn into(self) -> String {
        match self {
            RTPorRTCP::Rtp => String::from("rtp"),
            RTPorRTCP::Rtcp => String::from("rtcp"),
        }
    }
}

impl RTPorRTCP {
    fn to_string(self) -> String {
        self.into()
    }
}


pub fn encryption_cap(ssrc: Option<u32>) -> Result<gst::Caps> {
    let (key, _mki) = get_key_and_mki();

    //channels=(int)2,format=(string)S16LE,media=(string)audio,payload=(int)96,clock-rate=(int)44100,encoding-name=(string)L24
    let mut caps = gst::Caps::builder("application/x-srtp")
        .field("srtp-key", &key)
        .field("srtp-cipher", &"aes-128-icm")
        .field("srtp-auth", &"hmac-sha1-80")
        .field("srtcp-cipher", &"aes-128-icm")
        .field("srtcp-auth", &"hmac-sha1-80");


    if let Some(ssrc) = ssrc {
        caps = caps.field("ssrc", &ssrc);
    }

    
    let caps = caps.build();

    Ok(caps)
}

fn get_key_and_mki() -> (gst::Buffer, gst::Buffer) {
    let key = {
        let cl = crate::RTP_KEY.clone();
        let arr = cl.as_bytes();
        let part = Vec::from_iter(arr[5..35].iter().cloned());
        let buff = gst::Buffer::from_slice(part);
        buff
    };

    let mki = {
        let cl = crate::RTP_MKI.clone();
        let arr = cl.as_bytes();
        let part = Vec::from_iter(arr[5..35].iter().cloned());
        let buff = gst::Buffer::from_slice(part);
        buff
    };

    (key, mki)
}

pub fn client_encryption(rtpbin: &gst::Element) -> Result<()> {
    let all_signals = vec![
        (RTPorRTCP::Rtp, DeOrEncoder::Decoder),
        (RTPorRTCP::Rtcp,  DeOrEncoder::Encoder),
        (RTPorRTCP::Rtcp,  DeOrEncoder::Decoder),
    ];

    encrypt_bin(&rtpbin, all_signals)
}

pub fn server_encryption(rtpbin: &gst::Element) -> Result<()> {

    let all_signals = vec![
        (RTPorRTCP::Rtp, DeOrEncoder::Encoder),
        (RTPorRTCP::Rtp, DeOrEncoder::Decoder),
        (RTPorRTCP::Rtcp,  DeOrEncoder::Encoder),
        (RTPorRTCP::Rtcp,  DeOrEncoder::Decoder),
    ];

    encrypt_bin(&rtpbin, all_signals)
}

fn encrypt_bin(rtpbin: &gst::Element, all_signals: Vec<(RTPorRTCP, DeOrEncoder)>) -> Result<()> {

    let (key, mki) = get_key_and_mki();
    
    for (signal_type, coder_type) in all_signals {

        let key_cloned = key.clone();
        let mki_cloned = mki.clone();
        let signal_type_cloned = signal_type.clone();
        let coder_type_cloned = coder_type.clone();

        let signal = format!("request-{}-{}", signal_type.to_string(), coder_type.to_string());

        let signal_cloned = signal.clone();

        
        rtpbin.connect(&signal, false, move |vars| {

            let session:u32 = vars[1].get().unwrap_or(0);
            debug!("setup an {} for session {}", signal_cloned, session);

            Some(callback(&signal_type_cloned, &coder_type_cloned, &key_cloned, &mki_cloned, &session).expect("this should never fail!"))
        });
    }
    
    Ok(())
} 

fn callback(pre: &RTPorRTCP, deoren: &DeOrEncoder, key: &gst::Buffer, mki: &gst::Buffer, session: &u32 ) -> Result<glib::Value> {

    let element = match deoren {
        DeOrEncoder::Encoder => {
            let element = make_element(
                "srtpenc"
                , None
            )?;
        
            
            let name = format!("{}_sink_{}", pre.clone().to_string(), session);

            debug!("request pad for {} {}", pre.clone().to_string(), name );
            element.request_pad_simple(&name);
            element.try_set_property_from_str("rtp-cipher", &"aes-128-icm" )?;
            element.try_set_property_from_str("rtp-auth", &"hmac-sha1-80" )?;
            element.try_set_property_from_str("rtcp-cipher", &"aes-128-icm" )?;
            element.try_set_property_from_str("rtcp-auth", &"hmac-sha1-80" )?;

            element.try_set_property("key", key )?;

            //element.try_set_property("mki", mki )?;
            element
        }
        DeOrEncoder::Decoder => {
            let element = make_element(
                "srtpdec"
                , None
            )?;

            element.connect("request-key", false, |vars| request_key_callback(vars[0].get().unwrap(), vars[1].get().unwrap()));

        
            //element.try_set_property("key", key )?;
            //element.try_set_property("mki", mki )?;
            element
        }
    };
   

    Ok(element.to_value())
}

fn request_key_callback(_srtpdec: &gst::Element, ssrc: u32) -> Option<glib::Value> {

    debug!("SSRC {} request an KEY so we generate caps for it", ssrc);

    let caps = encryption_cap(Some(ssrc)).unwrap();

    Some(caps.to_value())
}