//! Desktop notifications. Sending happens on a throwaway thread so a slow or
//! missing notification daemon never stalls the interface.

use crate::config::APP_TITLE;

pub fn send(summary: impl Into<String>, body: impl Into<String>) {
    let (summary, body) = (summary.into(), body.into());
    std::thread::spawn(move || {
        let _ = notify_rust::Notification::new()
            .appname(APP_TITLE)
            .icon("pomolog")
            .summary(&summary)
            .body(&body)
            .show();
    });
}
