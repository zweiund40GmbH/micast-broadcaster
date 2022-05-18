# RTP Server with time Synchronization

This Library work as an RTP Radio Station with Network-Time Synchronization.

## Example

- run simple example to start a rtp server. be sure to change the audio file path's in simple.rs
- run example player to start receive the stream.

on each example the IP's are hardcoded so the __IPs Should be changed__ accordingly.

__more documentation and a better README if time is left.__

## TODO:

- no interval in scheduler
- better scheduler runtime performance
- From Utc to Nativetime in Scheduler


## Appendix

Heavly using [GStreamer](https://gitlab.freedesktop.org/gstreamer) and [GStreamer-rs](https://gitlab.freedesktop.org/gstreamer/gstreamer-rs/-/tree/main)
