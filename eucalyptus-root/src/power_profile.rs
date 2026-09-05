use futures::{Sink, SinkExt, Stream, StreamExt, join, lock::Mutex};
use zbus::{Connection, proxy, zvariant};

pub enum Message {
    NewActiveProfile(String),
    NewProfiles(Vec<String>),
}

pub enum Action {
    SetActiveProfile(String),
}

pub async fn task<Tx, Rx>(mut tx: Tx, mut rx: Rx) -> Result<(), String>
where
    Tx: Sink<Message> + Clone + Unpin,
    Rx: Stream<Item = Action> + Unpin,
{
    let connection = Connection::system()
        .await
        .map_err(|e| format!("Failed to connect to system bus: {e}"))?;
    let proxy = PowerProfilesProxy::new(&connection)
        .await
        .map_err(|e| format!("Failed to create properties proxy: {e}"))?;
    // let connection = match Connection::system().await {
    //     Ok(x) => x,
    //     Err(e) => {
    //         let _ = tx.send(Message::Error(format!("Failed to connect to system bus: {e}"))).await;
    //         tracing::error!(error = %e, "Failed to connect to system bus");
    //         return;
    //     }
    // };
    // let proxy = match PowerProfilesProxy::new(&connection).await {
    //     Ok(x) => x,
    //     Err(e) => {
    //         tx.send(Message::Error(format!("Failed to create properties proxy: {e}"))).await;
    //         tracing::error!(error = %e, "Failed to create properties proxy");
    //         return;
    //     }
    // };
    let mut active_profile_stream = proxy.receive_active_profile_changed().await;
    let active_profile = Mutex::new(None);
    let mut profiles_stream = proxy.receive_profiles_changed().await;
    let profiles = Mutex::new(None);

    // TODO: use select!() and return error?
    join!(
        {
            let mut tx = tx.clone();
            async move {
                while let Some(new_active_profile) = active_profile_stream.next().await {
                    match new_active_profile.get().await {
                        Ok(new_active_profile) => {
                            tracing::info!(new_active_profile, "Power profile changed");
                            let _ = tx
                                .send(Message::NewActiveProfile(new_active_profile.clone()))
                                .await;
                            active_profile.lock().await.replace(new_active_profile);
                        }
                        Err(e) => {
                            tracing::error!(error = %e, "Failed to get new ActiveProfile");
                        }
                    }
                }
                tracing::warn!("Receive ActiveProfile stream ended");
            }
        },
        async {
            while let Some(new_profiles) = profiles_stream.next().await {
                match new_profiles.get().await {
                    Ok(new_profiles) => {
                        tracing::info!(?new_profiles, "Power profile changed");
                        let new_profiles = new_profiles
                            .into_iter()
                            .map(|Profile { profile }| profile)
                            .collect::<Vec<_>>();
                        let _ = tx.send(Message::NewProfiles(new_profiles.clone())).await;
                        profiles.lock().await.replace(new_profiles);
                    }
                    Err(e) => {
                        tracing::error!(error = %e, "Failed to get new ActiveProfile");
                    }
                }
            }
            tracing::warn!("Receive ActiveProfile stream ended");
        },
        async {
            while let Some(task) = rx.next().await {
                let Action::SetActiveProfile(profile) = task;
                if let Err(e) = proxy.set_active_profile(&profile).await {
                    tracing::error!(error = %e, "Failed to set active profile");
                }
            }
        },
    );
    Ok(())
}

// <https://upower.pages.freedesktop.org/power-profiles-daemon/gdbus-org.freedesktop.UPower.PowerProfiles.html>
#[proxy(
    interface = "org.freedesktop.UPower.PowerProfiles",
    default_service = "org.freedesktop.UPower.PowerProfiles",
    default_path = "/org/freedesktop/UPower/PowerProfiles"
)]
trait PowerProfiles {
    fn hold_profile(&self, profile: &str, reason: &str, application_id: &str) -> zbus::Result<u32>;
    fn release_profile(&self, cookie: u32) -> zbus::Result<()>;
    fn set_action_enabled(&self, action: &str, enabled: bool) -> zbus::Result<()>;

    #[zbus(signal)]
    fn profile_released(&self, cookie: u32) -> zbus::Result<()>;

    #[zbus(property)]
    fn active_profile(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn set_active_profile(&self, active_profile: &str) -> zbus::Result<()>;
    #[zbus(property)]
    fn performance_degraded(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn profiles(&self) -> zbus::Result<Vec<Profile>>;
}

#[derive(Debug, zvariant::Value)]
#[zvariant(signature = "a{sv}")]
struct Profile {
    #[zvariant(rename = "Profile")]
    profile: String,
}
