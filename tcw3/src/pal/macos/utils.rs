// Based on <https://github.com/rust-windowing/winit/blob/master/src/platform/macos/window.rs>
use cocoa::{
    base::{id, nil},
    foundation::NSAutoreleasePool,
};
use objc::{
    class, msg_send,
    runtime::{Object, Sel, BOOL, NO, YES},
    sel, sel_impl,
};
use std::ops::Deref;

pub struct IdRef(id);

impl IdRef {
    pub fn new(i: id) -> IdRef {
        IdRef(i)
    }

    #[allow(dead_code)]
    pub fn retain(i: id) -> IdRef {
        if i != nil {
            let _: id = unsafe { msg_send![i, retain] };
        }
        IdRef(i)
    }

    pub fn non_nil(self) -> Option<IdRef> {
        if self.0 == nil {
            None
        } else {
            Some(self)
        }
    }
}

impl Drop for IdRef {
    fn drop(&mut self) {
        if self.0 != nil {
            with_autorelease_pool(|| unsafe {
                let _: () = msg_send![self.0, release];
            });
        }
    }
}

impl Deref for IdRef {
    type Target = id;
    fn deref<'a>(&'a self) -> &'a id {
        &self.0
    }
}

unsafe impl owning_ref::StableAddress for IdRef {}

impl Clone for IdRef {
    fn clone(&self) -> IdRef {
        if self.0 != nil {
            let _: id = unsafe { msg_send![self.0, retain] };
        }
        IdRef(self.0)
    }
}

extern "C" fn yes(_: &Object, _: Sel) -> BOOL {
    YES
}

pub fn with_autorelease_pool<T>(f: impl FnOnce() -> T) -> T {
    unsafe {
        let autoreleasepool = NSAutoreleasePool::new(nil);
        let result = f();
        let _: () = msg_send![autoreleasepool, release];
        result
    }
}

pub fn is_main_thread() -> bool {
    let result: BOOL = unsafe { msg_send![class!(NSThread), isMainThread] };
    result != NO
}

pub fn ensure_main_thread() {
    assert!(
        is_main_thread(),
        "this operation is only valid for a main thread"
    );
}
