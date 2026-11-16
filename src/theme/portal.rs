//! The desktop portal's appearance settings: the one place every desktop
//! agrees on for "prefer dark".

use std::sync::OnceLock;
use zbus::blocking::Connection;
use zbus::zvariant::OwnedValue;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scheme {
    NoPreference,
    Dark,
    Light,
}

fn connection() -> Option<&'static Connection> {
    static CONN: OnceLock<Option<Connection>> = OnceLock::new();
    CONN.get_or_init(|| Connection::session().ok()).as_ref()
}

fn read(key: &str) -> Option<OwnedValue> {
    let reply = connection()?
        .call_method(
            Some("org.freedesktop.portal.Desktop"),
            "/org/freedesktop/portal/desktop",
            Some("org.freedesktop.portal.Settings"),
            "ReadOne",
            &("org.freedesktop.appearance", key),
        )
        .ok()?;
    reply.body().deserialize::<OwnedValue>().ok()
}

/// None when no portal is running.
pub fn color_scheme() -> Option<Scheme> {
    let v = read("color-scheme")?;
    let n = u32::try_from(v).ok()?;
    Some(match n {
        1 => Scheme::Dark,
        2 => Scheme::Light,
        _ => Scheme::NoPreference,
    })
}
