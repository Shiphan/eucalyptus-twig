use smithay_client_toolkit::shell::wlr_layer;

pub enum Action<T> {
    Output(T),
    OpenWindow(WindowSettings),
    CloseAllWindow,
    Exit,
}

impl<T> Action<T> {
    pub fn map_output<U>(self, f: &mut impl FnMut(T) -> U) -> Action<U> {
        match self {
            Action::Output(output) => Action::Output(f(output)),
            Action::OpenWindow(window_settings) => Action::OpenWindow(window_settings),
            Action::CloseAllWindow => Action::CloseAllWindow,
            Action::Exit => Action::Exit,
        }
    }
}

impl<T> From<T> for Action<T> {
    fn from(value: T) -> Self {
        Self::Output(value)
    }
}

#[derive(Clone)]
pub struct WindowSettings {
    pub open_on_every_output: bool,
    pub layer: wlr_layer::Layer,
    pub namespace: Option<String>,
    pub anchor: wlr_layer::Anchor,
    pub exclusive_zone: bool,
}
