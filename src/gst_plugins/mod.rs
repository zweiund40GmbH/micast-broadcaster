// Copyright (C) 2021 OneStream Live <guillaume.desmottes@onestream.live>
//
// This Source Code Form is subject to the terms of the Mozilla Public License, v2.0.
// If a copy of the MPL was not distributed with this file, You can obtain one at
// <https://mozilla.org/MPL/2.0/>.
//
// SPDX-License-Identifier: MPL-2.0
#![allow(clippy::non_send_fields_in_send_ty)]

use gstreamer as gst;
use gst::glib;

pub mod uriplaylistbin;

fn plugin_init(plugin: &gst::Plugin) -> Result<(), glib::BoolError> {
    uriplaylistbin::register(plugin)?;
    Ok(())
}

gst::plugin_define!(
    uriplaylistbin,
    "Package Description for uriplaylistbin",
    plugin_init,
    env!("CARGO_PKG_VERSION"),
    // FIXME: MPL-2.0 is only allowed since 1.18.3 (as unknown) and 1.20 (as known)
    "MPL",
    "gst_uriplaylistbin",
    "gst_uriplaylistbin",
    "CARGO_PKG_REPOSITORY",
    "BUILD_REL_DATE"
);

