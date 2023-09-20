use crate::D3DResult;
use std::{
    ffi::c_void,
    fmt,
    hash::{Hash, Hasher},
    ops::Deref,
};
use winapi::{um::unknwnbase::IUnknown, Interface};

/// A nullable pointer to a COM object.
///
/// # Invariants
///
/// This data structure contains one of the following:
///
/// 1. A null pointer.
/// 2. A pointer to a valid instance of a COM object that implements `T`.
#[repr(transparent)]
pub struct ComPtr<T: Interface>(*mut T);

impl<T: Interface> ComPtr<T> {
    /// Create a ComPtr from a raw pointer. This will _not_ call AddRef on the pointer, assuming
    /// that it has already been called.
    ///
    /// # Safety
    ///
    /// - `raw` must be a valid pointer to a COM object that implements T.
    pub unsafe fn from_reffed(raw: *mut T) -> Self {
        debug_assert!(!raw.is_null());
        ComPtr(raw)
    }

    pub fn null() -> Self {
        ComPtr(std::ptr::null_mut())
    }

    /// Constructs a tracked COM pointer from `raw`, calling [`AddRef`] on it.
    ///
    /// # Safety
    ///
    /// This constructor is unsound to use unless [invariants for this data structure][self] are
    /// maintained by `raw`.
    ///
    /// [`AddRef`]: https://learn.microsoft.com/en-us/windows/win32/api/unknwn/nf-unknwn-iunknown-addref
    pub unsafe fn from_raw(raw: *mut T) -> Self {
        debug_assert!(!raw.is_null());
        (*raw.cast::<IUnknown>()).AddRef();
        ComPtr(raw)
    }

    /// Returns true if the inner pointer is null.
    pub fn is_null(&self) -> bool {
        self.0.is_null()
    }

    /// Returns the raw inner pointer as mutable. May be null.
    pub fn as_mut_ptr(&self) -> *mut T {
        self.0
    }

    /// Returns a pointer to the inner pointer, casted to [`c_void`].
    ///
    /// Useful for D3D functions that initialize objects with C's `void**` as an out parameter.
    ///
    /// # Safety
    ///
    /// This method is not `unsafe` by itself. However, readers should remember that it is unsound
    /// to assign to the pointee returned here unless [invariants for this data structure][self]
    /// are maintained.
    pub fn mut_void(&mut self) -> *mut *mut c_void {
        &mut self.0 as *mut *mut _ as *mut *mut _
    }

    /// Returns a pointer to the inner pointer (of `T`).
    ///
    /// Useful for D3D functions that initialize objects with `T**` as an out parameter.
    ///
    /// # Safety
    ///
    /// This method is not `unsafe` by itself. However, readers should remember that it is unsound
    /// to assign to the pointee returned here unless [invariants for this data structure][self]
    /// are maintained.
    pub fn mut_self(&mut self) -> *mut *mut T {
        &mut self.0 as *mut *mut _
    }
}

impl<T: Interface> ComPtr<T>
where
    T: Interface,
{
    /// Returns a reference to the inner pointer, casted to [`IUnknown`].
    ///
    /// # Safety
    ///
    /// - This pointer must not be null.
    pub unsafe fn as_unknown(&self) -> &IUnknown {
        debug_assert!(!self.is_null());
        &*(self.0 as *mut IUnknown)
    }

    /// Returns a reference to the inner pointer casted as a pointer to [`IUnknown`].
    pub fn as_unknown(&self) -> &IUnknown {
        unsafe { &*(self.0.cast()) }
    }


    /// Casts the `T` to `U` using `QueryInterface` (AKA [`Interface`]).
    ///
    /// # Safety
    ///
    /// - This pointer must not be null.
    ///
    /// [`QueryInterface`]: https://learn.microsoft.com/en-us/windows/win32/api/unknwn/nf-unknwn-iunknown-queryinterface(refiid_void)
    pub unsafe fn cast<U>(&self) -> D3DResult<ComPtr<U>>
    where
        U: Interface,
    {
        debug_assert!(!self.is_null());
        let mut obj = ComPtr::<U>::null();
        let hr = self
            .as_unknown()
            .QueryInterface(&U::uuidof(), obj.mut_void());
        (obj, hr)
    }

    /// Attempts to cast `T` to `U` using `QueryInterface`.
    pub fn cast<U>(&self) -> D3DResult<Option<ComPtr<U>>>
    where
        U: Interface,
    {
        let mut obj = std::ptr::null_mut();
        let unknown = unsafe { self.as_unknown() };
        // SAFETY: All COM pointers implement `IUnknown`; `unknown` should therefore be valid as an
        // invariant of this type.
        let hr = unsafe { unknown.QueryInterface(&U::uuidof(), &mut obj) };
        // SAFETY: `obj` is either a valid COM pointer to `U` in the case of success, or `null`.
        let obj = (!obj.is_null()).then(|| unsafe { ComPtr::from_reffed(obj.cast()) });
        (obj, hr)
    }
}

impl<T: Interface> Clone for ComPtr<T> {
    fn clone(&self) -> Self {
        debug_assert!(!self.is_null());
        unsafe {
            self.as_unknown().AddRef();
        }
        ComPtr(self.0)
    }
}

impl<T: Interface> Drop for ComPtr<T> {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                self.as_unknown().Release();
            }
        }
    }
}

impl<T: Interface> Drop for ComPtr<T> {
    fn drop(&mut self) {
        unsafe {
            self.as_unknown().Release();
        }
    }
}

impl<T: Interface> Deref for ComPtr<T> {
    type Target = T;
    fn deref(&self) -> &T {
        assert!(!self.is_null());
        unsafe { &*self.0 }
    }
}

impl<T: Interface> Deref for ComPtr<T> {
    type Target = T;
    fn deref(&self) -> &T {
        unsafe { &*self.0 }
    }
}

impl<T: Interface> fmt::Debug for ComPtr<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "ComPtr( ptr: {:?} )", self.0)
    }
}

impl<T: Interface> PartialEq<*mut T> for ComPtr<T> {
    fn eq(&self, other: &*mut T) -> bool {
        self.0 == *other
    }
}

impl<T: Interface> PartialEq for ComPtr<T> {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl<T: Interface> Hash for ComPtr<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.hash(state);
    }
}

/// Macro that allows generation of an easy to use enum for dealing with many different possible versions of a COM object.
///
/// Give the variants so that parents come before children. This often manifests as going up in order (1 -> 2 -> 3). This is vital for safety.
///
/// Three function names need to be attached to each variant. The examples are given for the MyComObject1 variant below:
/// - the from function (`ComPtr<actual::ComObject1> -> Self`)
/// - the as function (`&self -> Option<ComPtr<actual::ComObject1>>`)
/// - the unwrap function (`&self -> ComPtr<actual::ComObject1>` panicing on failure to cast)
///
/// ```rust
/// # pub use d3d12::weak_com_inheritance_chain;
/// # mod actual {
/// #     pub struct ComObject; impl winapi::Interface for ComObject { fn uuidof() -> winapi::shared::guiddef::GUID { todo!() } }
/// #     pub struct ComObject1; impl winapi::Interface for ComObject1 { fn uuidof() -> winapi::shared::guiddef::GUID { todo!() } }
/// #     pub struct ComObject2; impl winapi::Interface for ComObject2 { fn uuidof() -> winapi::shared::guiddef::GUID { todo!() } }
/// # }
/// weak_com_inheritance_chain! {
///     pub enum MyComObject {
///         MyComObject(actual::ComObject), from_my_com_object, as_my_com_object, my_com_object; // First variant doesn't use "unwrap" as it can never fail
///         MyComObject1(actual::ComObject1), from_my_com_object1, as_my_com_object1, unwrap_my_com_object1;
///         MyComObject2(actual::ComObject2), from_my_com_object2, as_my_com_object2, unwrap_my_com_object2;
///     }
/// }
/// ```
#[macro_export]
macro_rules! weak_com_inheritance_chain {
    // We first match a human readable enum style, before going into the recursive section.
    //
    // Internal calls to the macro have either the prefix
    // - @recursion_logic for the recursion and termination
    // - @render_members for the actual call to fill in the members.
    (
        $(#[$meta:meta])*
        $vis:vis enum $name:ident {
            $first_variant:ident($first_type:ty), $first_from_name:ident, $first_as_name:ident, $first_unwrap_name:ident $(;)?
            $($variant:ident($type:ty), $from_name:ident, $as_name:ident, $unwrap_name:ident);* $(;)?
        }
    ) => {
        $(#[$meta])*
        $vis enum $name {
            $first_variant($crate::ComPtr<$first_type>),
            $(
                $variant($crate::ComPtr<$type>)
            ),+
        }
        impl $name {
            $crate::weak_com_inheritance_chain! {
                @recursion_logic,
                $vis,
                ;
                $first_variant($first_type), $first_from_name, $first_as_name, $first_unwrap_name;
                $($variant($type), $from_name, $as_name, $unwrap_name);*
            }
        }

        impl std::ops::Deref for $name {
            type Target = $crate::ComPtr<$first_type>;
            fn deref(&self) -> &Self::Target {
                self.$first_unwrap_name()
            }
        }
    };

    // This is the iteration case of the recursion. We instantiate the member functions for the variant we
    // are currently at, recursing on ourself for the next variant. Note we only keep track of the previous
    // variant name, not the functions names, as those are not needed.
    (
        @recursion_logic,
        $vis:vis,
        $(,)? $($prev_variant:ident),* $(,)?;
        $this_variant:ident($this_type:ty), $this_from_name:ident, $this_as_name:ident, $this_unwrap_name:ident $(;)?
        $($next_variant:ident($next_type:ty), $next_from_name:ident, $next_as_name:ident, $next_unwrap_name:ident);*
    ) => {
        // Actually generate the members for this variant. Needs the previous and future variant names.
        $crate::weak_com_inheritance_chain! {
            @render_members,
            $vis,
            $this_from_name, $this_as_name, $this_unwrap_name;
            $($prev_variant),*;
            $this_variant($this_type);
            $($next_variant),*;
        }

        // Recurse on ourselves. If there is no future variants left, we'll hit the base case as the final expansion returns no tokens.
        $crate::weak_com_inheritance_chain! {
            @recursion_logic,
            $vis,
            $($prev_variant),* , $this_variant;
            $($next_variant($next_type), $next_from_name, $next_as_name, $next_unwrap_name);*
        }
    };
    // Base case for recursion. There are no more variants left
    (
        @recursion_logic,
        $vis:vis,
        $($prev_variant:ident),*;
    ) => {};


    // This is where we generate the members using the given names.
    (
        @render_members,
        $vis:vis,
        $from_name:ident, $as_name:ident, $unwrap_name:ident;
        $($prev_variant:ident),*;
        $variant:ident($type:ty);
        $($next_variant:ident),*;
    ) => {
        // Construct this enum from weak pointer to this interface. For best usability, always use the highest constructor you can. This doesn't try to upcast.
        $vis unsafe fn $from_name(value: $crate::ComPtr<$type>) -> Self {
            Self::$variant(value)
        }

        // Returns Some if the value implements the interface otherwise returns None.
        $vis fn $as_name(&self) -> Option<&$crate::ComPtr<$type>> {
            match *self {
                $(
                    Self::$prev_variant(_) => None,
                )*
                Self::$variant(ref v) => Some(v),
                $(
                    Self::$next_variant(ref v) => {
                        // v is &ComPtr<NextType> and se cast to &ComPtr<Type>
                        Some(unsafe { std::mem::transmute(v) })
                    }
                )*
            }
        }

        // Returns the interface if the value implements it, otherwise panics.
        #[track_caller]
        $vis fn $unwrap_name(&self) -> &$crate::ComPtr<$type> {
            match *self {
                $(
                    Self::$prev_variant(_) => panic!(concat!("Tried to unwrap a ", stringify!($prev_variant), " as a ", stringify!($variant))),
                )*
                Self::$variant(ref v) => &*v,
                $(
                    Self::$next_variant(ref v) => {
                        // v is &ComPtr<NextType> and se cast to &ComPtr<Type>
                        unsafe { std::mem::transmute(v) }
                    }
                )*
            }
        }
    };
}
