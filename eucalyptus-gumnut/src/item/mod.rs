use clap::ValueEnum;
use eucalyptus_cellulose::{Element, task::Task};
use iced_futures::Subscription;

// pub mod backlight;
pub mod power_profile;
// pub mod volume;

#[derive(Clone, ValueEnum)]
pub enum ItemKind {
    Backlight,
    Media,
    PowerProfile,
    Volume,
}

pub trait Item: Sized {
    type Config;
    type Message;

    fn new(config: &Self::Config) -> (Self, Task<Self::Message>);

    fn update(&mut self, message: Self::Message) -> impl Into<Task<Self::Message>>;

    fn view(&self) -> Element<'_, Self::Message>;

    fn subscription(&self) -> impl Into<Subscription<Self::Message>> {
        Subscription::none()
    }
}
