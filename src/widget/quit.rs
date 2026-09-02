use iced_runtime::Task;

use crate::{application::Element, widget::{Widget, WidgetPadding}};

// FIXME: segmentation fault (core dumped) after iced_runtime::exit()

pub struct Quit;

impl Widget for Quit {
    type Config = ();

    type Message = ();

    fn new((): &Self::Config) -> (Self, Task<Self::Message>) {
        (Self, Task::none())
    }

    fn update(&mut self, (): Self::Message) -> impl Into<Task<Self::Message>> {
        iced_runtime::exit()
    }

    fn view(&self) -> Element<'_, Self::Message> {
        iced_widget::mouse_area(iced_widget::container(iced_widget::text("Quit")).widget_padding()).on_press(()).into()
    }
}
