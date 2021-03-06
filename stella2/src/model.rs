use harmony::{set_field, Elem};
use miniserde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub struct AppState {
    pub main_wnd: Elem<WndState>,
    /// Indicates whether the Preferences window is visible.
    pub pref_visible: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WndState {
    // UI state - It could be a local state of widget controllers, but we store
    // it here instead so that it can be intercepted by a persistence middleware
    pub sidebar_width: f32,
    pub editor_height: f32,
    pub sidebar_visible: bool,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            main_wnd: Elem::new(WndState {
                sidebar_width: 200.0,
                editor_height: 50.0,
                sidebar_visible: true,
            }),
            pref_visible: false,
        }
    }
}

#[derive(Debug, Clone)]
pub enum AppAction {
    Wnd(WndAction),
    /// Hides the Preferences window.
    HidePref,
    /// Toggles the visibility of the Preferences window.
    TogglePref,
}

#[derive(Debug, Clone)]
pub enum WndAction {
    SetSidebarWidth(f32),
    SetEditorHeight(f32),
    ToggleSidebar,
}

impl AppState {
    pub fn reduce(this: Elem<Self>, action: &AppAction) -> Elem<Self> {
        match action {
            AppAction::Wnd(wnd_action) => set_field! {
                main_wnd: WndState::reduce(Elem::clone(&this.main_wnd), wnd_action),
                ..this
            },
            AppAction::HidePref => set_field! {
                pref_visible: false,
                ..this
            },
            AppAction::TogglePref => set_field! {
                pref_visible: !this.pref_visible,
                ..this
            },
        }
    }
}

impl WndState {
    fn reduce(this: Elem<Self>, action: &WndAction) -> Elem<Self> {
        match action {
            WndAction::SetSidebarWidth(x) => set_field! {
                sidebar_width: *x,
                ..this
            },
            WndAction::SetEditorHeight(x) => set_field! {
                editor_height: *x,
                ..this
            },
            WndAction::ToggleSidebar => set_field! {
                sidebar_visible: !this.sidebar_visible,
                ..this
            },
        }
    }
}
