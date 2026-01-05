use std::{ops::Deref, time::Duration};

use gpui::{
    Animation, AnimationExt, App, Context, Entity, FocusHandle, KeyBinding, PlatformDisplay,
    StatefulInteractiveElement, Window, WindowBackgroundAppearance, WindowKind, WindowOptions,
    actions, black, div, ease_in_out,
    layer_shell::{KeyboardInteractivity, Layer, LayerShellOptions},
    prelude::*,
    relative, rems, white,
};

actions!([Escape]);

pub struct PowerMenu {
    selected: Option<PowerMenuOption>,
    focus_handle: FocusHandle,
}

impl PowerMenu {
    pub fn build_root_view(window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| {
            cx.bind_keys([
                KeyBinding::new("escape", Escape, Some("power-menu")),
                KeyBinding::new("q", Escape, Some("power-menu")),
            ]);

            // TODO: on_action callback on an element requires that element to be focused,
            // should see if there is any way to bind a key on window level
            let focus_handle = cx.focus_handle();
            focus_handle.focus(window, cx);

            Self {
                selected: None,
                focus_handle,
            }
        })
    }
    pub fn window_options(
        display: Option<impl Deref<Target = impl PlatformDisplay + ?Sized>>,
    ) -> WindowOptions {
        let window_bounds = display
            .as_ref()
            .map(|x| gpui::WindowBounds::Windowed(x.bounds()));
        tracing::info!(?window_bounds);
        WindowOptions {
            window_bounds,
            titlebar: None,
            kind: WindowKind::LayerShell(LayerShellOptions {
                namespace: "eucalyptus-twig-power-menu".to_owned(),
                layer: Layer::Overlay,
                keyboard_interactivity: KeyboardInteractivity::Exclusive,
                ..Default::default()
            }),
            display_id: display.as_ref().map(|x| x.id()),
            window_background: WindowBackgroundAppearance::Transparent,
            ..Default::default()
        }
    }
}

impl Render for PowerMenu {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let wrapper = div()
            .id("power-menu-wrapper")
            .key_context("power-menu")
            .track_focus(&self.focus_handle)
            .on_action(|_escape: &Escape, window, _cx| {
                window.remove_window();
            })
            .on_click(|_, window, _| {
                window.remove_window();
            })
            .size_full()
            .flex()
            // .flex_col()
            .items_center()
            .justify_center()
            .gap(rems(0.5));
        // .bg(opaque_grey(0.2, 0.8));

        let button = || {
            div()
                .flex()
                .items_center()
                .justify_center()
                .rounded_xl()
                .text_size(rems(5.0))
                .text_color(white())
                .font_family("Material Symbols Rounded")
                .bg(black())
        };

        if let Some(selected_option) = self.selected {
            wrapper
                .child(
                    button()
                        .id("power-menu-back")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.selected = None;
                            cx.stop_propagation();
                        }))
                        .px(rems(0.6))
                        .child(""), // .with_animation(
                                     //     "power-menu-back-name-animation",
                                     //     Animation::new(Duration::from_millis(1500))
                                     //         .with_easing(ease_in_out),
                                     //     |element, delta| element.w(relative(delta)),
                                     // ),
                )
                .child(
                    button()
                        .id("power-menu-real")
                        .on_click(|_, window, cx| {
                            window.remove_window();
                            cx.stop_propagation();
                        })
                        .gap(rems(2.0))
                        .px(rems(2.0))
                        .child(selected_option.icon())
                        .child(
                            div()
                                .text_size(rems(3.6))
                                .font_family("Noto Sans")
                                .child(selected_option.name())
                                .with_animation(
                                    "power-menu-real-name",
                                    Animation::new(Duration::from_millis(1500))
                                        .with_easing(ease_in_out),
                                    |element, delta| element.w(relative(delta)),
                                ),
                        ),
                )
        } else {
            wrapper.children(PowerMenuOption::ALL.map(|option| {
                button()
                    .id(format!("power-menu-option-{}", option.id()))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.selected = Some(option);
                        cx.stop_propagation();
                    }))
                    .w(rems(8.0))
                    .child(option.icon())
            }))
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum PowerMenuOption {
    Lock,
    Suspend,
    Hibernate,
    Reboot,
    Shutdown,
}

impl PowerMenuOption {
    const ALL: [Self; 5] = [
        Self::Lock,
        Self::Suspend,
        Self::Hibernate,
        Self::Reboot,
        Self::Shutdown,
    ];
    const fn id(&self) -> &'static str {
        match self {
            Self::Lock => "lock",
            Self::Suspend => "suspend",
            Self::Hibernate => "hibernate",
            Self::Reboot => "reboot",
            Self::Shutdown => "shutdown",
        }
    }
    const fn name(&self) -> &'static str {
        match self {
            Self::Lock => "Lock",
            Self::Suspend => "Suspend",
            Self::Hibernate => "Hibernate",
            Self::Reboot => "Reboot",
            Self::Shutdown => "Shutdown",
        }
    }
    const fn icon(&self) -> &'static str {
        match self {
            Self::Lock => "󰌿",
            Self::Suspend => "󰏥",
            Self::Hibernate => "󰤄",
            Self::Reboot => "󰜉",
            Self::Shutdown => "󰐥",
        }
    }
}
