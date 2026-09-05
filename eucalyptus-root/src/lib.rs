use futures::{FutureExt, StreamExt};

pub mod audio;
pub mod mpris;
pub mod power_profile;
pub mod screen_brightness;

pub fn stream<T>(
    f: impl AsyncFnOnce(futures::channel::mpsc::UnboundedSender<T>),
) -> impl futures::Stream<Item = T> {
    let (tx, rx) = futures::channel::mpsc::unbounded();
    let task = f(tx).into_stream().filter_map(async |()| None);
    futures::stream::select(task, rx)
}
