use super::traits::{WndListener, WM};
use std::rc::Rc;

#[derive(Clone)]
pub struct WndAttrs<T: WM, TCaption> {
    pub size: Option<[u32; 2]>,
    pub caption: Option<TCaption>,
    pub visible: Option<bool>,
    pub listener: Option<Rc<dyn WndListener<T>>>,
}

impl<T: WM, TCaption> Default for WndAttrs<T, TCaption> {
    fn default() -> Self {
        Self {
            size: None,
            caption: None,
            visible: None,
            listener: None,
        }
    }
}

impl<T: WM, TCaption> WndAttrs<T, TCaption>
where
    TCaption: AsRef<str>,
{
    pub fn as_ref(&self) -> WndAttrs<T, &str> {
        WndAttrs {
            size: self.size,
            caption: self.caption.as_ref().map(AsRef::as_ref),
            visible: self.visible,
            listener: self.listener.clone(),
        }
    }
}
