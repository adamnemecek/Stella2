use harmony::{set_field, Elem};

#[derive(Debug, Clone)]
pub struct AppState {
    pub main_wnd: Elem<WndState>,
}

#[derive(Debug, Clone)]
pub struct WndState {
    // UI state - It could be a local state of widget controllers, but we store
    // it here instead so that it can be intercepted by a persistence middleware
    pub sidebar_width: f32,
    pub editor_height: f32,
}

impl AppState {
    // TODO: Restore session state
    pub fn new() -> Self {
        Self {
            main_wnd: Elem::new(WndState {
                sidebar_width: 200.0,
                editor_height: 50.0,
            }),
        }
    }
}

#[derive(Debug, Clone)]
pub enum AppAction {
    Wnd(WndAction),
}

#[derive(Debug, Clone)]
pub enum WndAction {
    SetSidebarWidth(f32),
    SetEditorHeight(f32),
}

impl AppState {
    pub fn reduce(this: Elem<Self>, action: &AppAction) -> Elem<Self> {
        match action {
            AppAction::Wnd(wnd_action) => set_field! {
                main_wnd: WndState::reduce(Elem::clone(&this.main_wnd), wnd_action),
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
        }
    }
}