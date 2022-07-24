#![allow(clippy::too_many_arguments)]
use std::{collections::HashMap, error::Error};

use zbus::zvariant::Value;
use zbus::{dbus_proxy, Connection};
#[dbus_proxy(
    interface = "org.freedesktop.Notifications",
    default_service = "org.freedesktop.Notifications",
    default_path = "/org/freedesktop/Notifications"
)]
trait Notifications {
    fn notify(
        &self,
        app_name: &str,
        replaces_id: u32,
        app_icon: &str,
        summary: &str,
        body: &str,
        actions: &[&str],
        hints: &HashMap<&str, &Value<'_>>,
        expire_timeout: i32,
    ) -> zbus::Result<u32>;
}

pub async fn notify(summary: &str, body: &str) -> Result<(), Box<dyn Error>> {
    let connection = Connection::session().await?;

    // `dbus_proxy` macro creates `NotificationProxy` based on `Notifications` trait.
    let proxy = NotificationsProxy::new(&connection).await?;
    let reply = proxy
        .notify(
            "transgression",
            0,
            "dialog-information",
            summary,
            body,
            &[],
            &HashMap::new(),
            5000,
        )
        .await?;
    dbg!(reply);

    Ok(())
}
