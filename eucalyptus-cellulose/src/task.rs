use futures::{FutureExt, StreamExt, future::IntoStream, stream::BoxStream};

use crate::action::Action;

pub type Task<T> = BoxStream<'static, Action<T>>;

pub trait TaskExt<T> {
    fn none() -> Self;
    fn future(future: impl Future<Output = T> + Send + 'static) -> Self;
    fn map_output<U>(self, f: impl FnMut(T) -> U + Send + 'static) -> Task<U>;
    fn discard<O>(self) -> Task<O>;
}

impl<T> TaskExt<T> for Task<T>
where
    T: Send + 'static,
{
    fn none() -> Self {
        futures::stream::empty().boxed()
    }

    fn future(future: impl Future<Output = T> + Send + 'static) -> Self {
        future.into_stream().map(Action::Output).boxed()
    }

    fn map_output<U>(self, mut f: impl FnMut(T) -> U + Send + 'static) -> Task<U> {
        self.map(move |action| action.map_output(&mut f)).boxed()
    }

    fn discard<O>(self) -> Task<O> {
        self.filter_map(async |_| None).boxed()
    }
}
