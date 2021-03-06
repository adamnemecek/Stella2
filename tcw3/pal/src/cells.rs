use std::{cell::UnsafeCell, fmt, marker::PhantomData, mem::ManuallyDrop};

use super::{prelude::WmTrait, Wm};

mod init;
pub use self::init::*;

/// Main-Thread Sticky — Like [`fragile::Sticky`], allows `!Send` types to be
/// moved between threads, but there are a few differences:
///
///  - The ownership is restricted to the main thread.
///  - When dropped, the inner value is sent back to the main thread and
///    destroyed in the main event loop.
///  - Provides additional methods for compile-time thread checking.
///
/// [`fragile::Sticky`]: https://docs.rs/fragile/0.3.0/fragile/struct.Sticky.html
pub struct MtSticky<T: 'static, TWM: WmTrait = Wm> {
    _phantom: PhantomData<TWM>,
    cell: ManuallyDrop<UnsafeCell<T>>,
}

unsafe impl<T: 'static, TWM: WmTrait> Send for MtSticky<T, TWM> {}
unsafe impl<T: 'static, TWM: WmTrait> Sync for MtSticky<T, TWM> {}

impl<T: 'static + fmt::Debug, TWM: WmTrait> fmt::Debug for MtSticky<T, TWM> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if let Ok(wm) = TWM::try_global() {
            f.debug_tuple("MtSticky")
                .field(self.get_with_wm(wm))
                .finish()
        } else {
            write!(f, "MtSticky(<not main thread>)")
        }
    }
}

#[allow(dead_code)]
impl<T: 'static, TWM: WmTrait> MtSticky<T, TWM> {
    /// Construct a `MtSticky` without thread checking.
    ///
    /// # Safety
    ///
    /// This method allows you to send an unsendable value to a main thread
    /// without checking the calling thread.
    ///
    /// The default values of many collection types are empty and thus safe to
    /// send to a main thread. Such types are annotated with [`SendInit`], and
    /// `MtSticky` implements `Init` when `T` is `SendInit`. *Consider using
    /// `<MtSticky as Init>::INIT` whenever possible.*
    /// See the following example:
    ///
    /// ```no_compile
    /// static WNDS: MtSticky<RefCell<WndPool>, Wm> = {
    ///     // `Wnd` is `!Send`, but there is no instance at this point, so this is safe
    ///     unsafe { MtSticky::new_unchecked(RefCell::new(LeakyPool::new())) }
    /// };
    /// // The above code can be replaced with:
    /// static WNDS: MtSticky<RefCell<WndPool>, Wm> = Init::INIT;
    /// ```
    #[inline]
    pub const unsafe fn new_unchecked(x: T) -> Self {
        Self {
            _phantom: PhantomData,
            cell: ManuallyDrop::new(UnsafeCell::new(x)),
        }
    }

    /// Construct a `MtSticky` containing a `Send`-able value.
    #[inline]
    pub const fn new(x: T) -> Self
    where
        T: Send,
    {
        unsafe { Self::new_unchecked(x) }
    }

    /// Construct a `MtSticky` with compile-time thread checking.
    #[inline]
    pub fn with_wm(_: TWM, x: T) -> Self {
        unsafe { Self::new_unchecked(x) }
    }

    /// Get a raw pointer to the inner value.
    #[inline]
    pub fn get_ptr(&self) -> *mut T {
        self.cell.get()
    }

    /// Take the inner value with run-time thread checking.
    #[inline]
    pub fn into_inner(self, _: TWM) -> T {
        let inner = unsafe { self.cell.get().read() };
        std::mem::forget(self);
        inner
    }

    /// Get a reference to the `Send`-able and `Sync` inner value.
    #[inline]
    pub fn get(&self) -> &T
    where
        T: Send + Sync,
    {
        unsafe { &*self.get_ptr() }
    }

    /// Get a reference to the `Send`-able inner value
    #[inline]
    pub fn get_mut(&mut self) -> &mut T
    where
        T: Send,
    {
        unsafe { &mut *self.get_ptr() }
    }

    /// Get a reference to the inner value with compile-time thread checking.
    #[inline]
    pub fn get_with_wm(&self, _: TWM) -> &T {
        unsafe { &*self.get_ptr() }
    }

    /// Get a mutable reference to the inner value with compile-time thread checking.
    #[inline]
    pub fn get_mut_with_wm(&mut self, _: TWM) -> &mut T {
        unsafe { &mut *self.get_ptr() }
    }
}

impl<T: 'static, TWM: WmTrait> Drop for MtSticky<T, TWM> {
    fn drop(&mut self) {
        if std::mem::needs_drop::<T>() {
            struct AssertSend<T>(T);
            unsafe impl<T> Send for AssertSend<T> {}

            // This is safe because the inner value was originally created
            // in the main thread, and we are sending it back to the main
            // thread.
            let cell = AssertSend(unsafe { self.cell.get().read() });
            TWM::invoke_on_main_thread(move |_| {
                drop(cell);
            });
        }
    }
}

/// Main-Thread Lock — Like `ReentrantMutex`, but only accessible to the main thread.
pub struct MtLock<T, TWM: WmTrait = Wm> {
    _phantom: PhantomData<TWM>,
    cell: UnsafeCell<T>,
}

unsafe impl<T: Send, TWM: WmTrait> Send for MtLock<T, TWM> {}
unsafe impl<T: Send, TWM: WmTrait> Sync for MtLock<T, TWM> {}

impl<T: fmt::Debug, TWM: WmTrait> fmt::Debug for MtLock<T, TWM> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if let Ok(wm) = TWM::try_global() {
            f.debug_tuple("MtLock").field(self.get_with_wm(wm)).finish()
        } else {
            write!(f, "MtLock(<not main thread>)")
        }
    }
}

#[allow(dead_code)]
impl<T, TWM: WmTrait> MtLock<T, TWM> {
    /// Construct a `MtLock`.
    #[inline]
    pub const fn new(x: T) -> Self {
        Self {
            _phantom: PhantomData,
            cell: UnsafeCell::new(x),
        }
    }

    /// Get a raw pointer to the inner value.
    #[inline]
    pub const fn get_ptr(&self) -> *mut T {
        self.cell.get()
    }

    /// Take the inner value.
    #[inline]
    pub fn into_inner(self) -> T {
        self.cell.into_inner()
    }

    /// Get a reference to the `Sync` inner value.
    #[inline]
    pub fn get(&self) -> &T
    where
        T: Sync,
    {
        unsafe { &*self.get_ptr() }
    }

    /// Get a mutably reference to the inner value.
    #[inline]
    pub fn get_mut(&mut self) -> &mut T {
        unsafe { &mut *self.get_ptr() }
    }

    /// Get a reference to the inner value with compile-time thread checking.
    #[inline]
    pub fn get_with_wm(&self, _: TWM) -> &T {
        unsafe { &*self.get_ptr() }
    }
}

/// A trait implemented by variables generated by the [`mt_lazy_static`] macro.
pub trait MtLazyStatic<TWM: WmTrait = Wm> {
    type Target;

    /// Initialize and get the inner value with compile-time thread checking.
    fn get_with_wm(&self, _: TWM) -> &Self::Target;

    /// Initialize and get the inner value without thread checking.
    ///
    /// # Safety
    ///
    /// The calling thread must be a main thread. Calling from a different
    /// thread is unsafe in many ways including:
    ///
    ///  - This method is implemented using `Wm::global_unchecked()`, which
    ///    includes the above condition as its prerequisite.
    ///
    ///  - This method provides unconditional access to the contents regardless
    ///    of the thread safety of the inner type. Getting a reference itself is
    ///    safe, but using it in some ways might cause undefined behaviour.
    ///
    unsafe fn get_unchecked(&self) -> &Self::Target {
        self.get_with_wm(TWM::global_unchecked())
    }
}

/// Like `lazy_static!`, but only accessible by the main thread. Can be used
/// for `!Send + !Sync` types. The defined variable implements [`MtLazyStatic`].
///
/// [`MtLazyStatic`]: crate::cells::MtLazyStatic
///
/// # Examples
///
/// ```
/// use tcw3_pal::{mt_lazy_static, prelude::*, LayerAttrs, HLayer};
/// # fn hoge(wm: tcw3_pal::Wm) {
/// mt_lazy_static! {
///     static <tcw3_pal::Wm> ref LAYER: HLayer =>
///         |wm| wm.new_layer(LayerAttrs::default());
/// }
///
/// let layer = LAYER.get_with_wm(wm);
/// # }
/// ```
///
/// `Wm` type defaults to `crate::Wm`, so the following example is equivalent
/// to the first one:
///
/// ```
/// use tcw3_pal::{mt_lazy_static, prelude::*, LayerAttrs, HLayer};
/// # fn hoge(wm: tcw3_pal::Wm) {
/// mt_lazy_static! {
///     static ref LAYER: HLayer => |wm| wm.new_layer(LayerAttrs::default());
/// }
///
/// let layer = LAYER.get_with_wm(wm);
/// # }
/// ```
#[macro_export]
macro_rules! mt_lazy_static {
    (
        $vis:vis static $(<$Wm:ty>)? ref $name:ident: $type:ty => $init:expr;
        $($rest:tt)*
    ) => {
        #[doc(hidden)]
        #[allow(non_camel_case_types)]
        $vis struct $name {
            cell: ::std::cell::UnsafeCell<::std::option::Option<$type>>,
            initing: ::std::cell::Cell<bool>,
        }

        unsafe impl Send for $name {}
        unsafe impl Sync for $name {}

        impl $name {
            #[cold]
            fn __init_cell(wm: $crate::WmOrDefault!($($Wm)*)) -> &'static $type {
                assert!(!$name.initing.get(), "recursion detected while lazily initializing a global variable");
                $name.initing.set(true);

                let initer: fn($crate::WmOrDefault!($($Wm)*)) -> $type = $init;

                let value = initer(wm);

                unsafe {
                    $name.cell.get().write(Some(value));
                    (&*($name.cell.get() as *const ::std::option::Option<$type>)).as_ref().unwrap()
                }
            }
        }

        impl $crate::prelude::MtLazyStatic<$crate::WmOrDefault!($($Wm)*)> for $name {
            type Target = $type;

            #[inline]
            fn get_with_wm(&self, wm: $crate::WmOrDefault!($($Wm)*)) -> &$type {
                unsafe {
                    if let Some(inner) = (*self.cell.get()).as_ref() {
                        inner
                    } else {
                        Self::__init_cell(wm)
                    }
                }
            }
        }

        $vis static $name: $name = $name {
            cell: ::std::cell::UnsafeCell::new(None),
            initing: ::std::cell::Cell::new(false),
        };

        $crate::mt_lazy_static! { $($rest)* }
    };
    () => {};
}

/// Expands to `crate::Wm` if input is empty.
#[doc(hidden)]
#[macro_export]
macro_rules! WmOrDefault {
    () => {
        $crate::Wm
    };
    ($t:ty) => {
        $t
    };
}
