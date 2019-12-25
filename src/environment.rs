//! Environment management utilities
//!
//! This module provide the tools to automatically bind the wayland global objects you need in your program.
//!
//! At the heart of this is the `environment!` macro, which allows you to signal the globals you need
//! and a struct to manage them as they are signaled in the registry.
//!
//! ## Global handlers
//!
//! Wayland globals are split in two kinds, that we will call here "single" globals and "multi" globals.
//!
//! - "single" globals represent a capability of the server. They are generally signaled in the registry
//!   from the start and never removed. They are signaled a single time. Examples of these globals are
//!   `wl_compositor`, `wl_shm` or `xdg_wm_base`.
//! - "multi" globals represent a resource that the server gives you access to. These globals can be
//!   created or removed during the run of the program, and may exist as more than one instance, each
//!   representing a different physical resource. Examples of such globals are `wl_output` or `wl_seat`.
//!
//! The objects you need to handle these globals must implement one the two traits
//! [`GlobalHandler<I>`](trait.GlobalHandler.html) or [`MultiGlobalHandler<I>`](trait.MultiGlobalHandler.html),
//! depending on the kind of globals it will handle. These objects are responsible for binding the globals
//! from the registry, and assigning them to filters to receive their events as necessary.
//!
//! This module provides a generic implementation of the [`GlobalHandler<I>`](trait.GlobalHandler.html) trait
//! as [`SimpleGlobal<I>`](struct.SimpleGlobal.html). It can manage "single" globals that do not generate
//! events, and thus require no filter.
//!
//! ## the environment macros
//!
//! Two macros are at the core of this module, used to create the initial
//! [`Environment<_>`](struct.Environment.html) instance. See their documentation for details about how to
//! use them: [`declare_environment!`](../macro.declare_environment.html) and
//! [`init_environment!`](../macro.init_environment.html).

use std::{cell::RefCell, rc::Rc};

use wayland_client::{protocol::wl_registry, Attached, GlobalManager, Interface, Proxy};

/*
 * Traits definitions
 */

/// Required trait for implementing a handler for "single" globals
pub trait GlobalHandler<I: Interface> {
    /// This global was created and signaled in the registry with given id and version
    fn created(&mut self, registry: Attached<wl_registry::WlRegistry>, id: u32, version: u32);
    /// Access the global if it was signaled
    fn get(&self) -> Option<Attached<I>>;
}

/// Required trait for implementing a handler for "multi" globals
pub trait MultiGlobalHandler<I: Interface> {
    /// A new instance of this global was created with given id and version
    fn created(&mut self, registry: Attached<wl_registry::WlRegistry>, id: u32, version: u32);
    /// The instance with given id was removed
    fn removed(&mut self, id: u32);
    /// Access all the currently existing instances
    fn get_all(&self) -> Vec<Attached<I>>;
}

/*
 * General Environment<E>
 */

/// A Wayland Environment
///
/// This struct is generated by the `environment!` macro, see module-level documentation
/// for more details about this.
///
/// This is the central point for accessing globals for your Wayland app. Any global that has
/// previously been declared in the `environment!` macro can be access from this type via the
/// `get_global`, `required_global` and `get_all_globals` methods.
///
/// This `Environment` is a handle that can be cloned.
pub struct Environment<E> {
    /// The underlying `GlobalManager`, if you need to do manual interaction with the
    /// registry. See `wayland-client` documentation for details.
    pub manager: GlobalManager,
    inner: Rc<RefCell<E>>,
}

impl<E> Environment<E> {
    #[doc(hidden)]
    pub fn wrap(manager: GlobalManager, inner: Rc<RefCell<E>>) -> Environment<E> {
        Environment { manager, inner }
    }

    /// Access a "single" global
    ///
    /// This method allows you to access any "single" global that has previously
    /// been declared in the `environment!` macro. It is forwarded to the `get()`
    /// method of the appropriate `GlobalHandler`.
    ///
    /// It returns `None` if the global has not (yet) been signaled by the registry.
    pub fn get_global<I: Interface>(&self) -> Option<Attached<I>>
    where
        E: GlobalHandler<I>,
    {
        self.inner.borrow().get()
    }

    /// Access a "single" global or panic
    ///
    /// This method is similar to `get_global`, but will panic with a detailed error
    /// message if the requested global was not advertized by the server.
    pub fn require_global<I: Interface>(&self) -> Attached<I>
    where
        E: GlobalHandler<I>,
    {
        match self.inner.borrow().get() {
            Some(g) => g,
            None => panic!("[SCTK] A missing global was required: {}", I::NAME),
        }
    }

    /// Access all instances of a "multi" global
    ///
    /// This will return a `Vec` containing all currently existing instances of the
    /// requested "multi" global that has been previously declared in the `environment!`
    /// macro. It is forwarded to the `get_all()` method of the appropriate
    /// `MultiGlobalHandler`.
    pub fn get_all_globals<I: Interface>(&self) -> Vec<Attached<I>>
    where
        E: MultiGlobalHandler<I>,
    {
        self.inner.borrow().get_all()
    }

    /// Access the extra values you stored into your environment type
    ///
    /// This gives your access, via a closure, to the values you stored
    /// into your environment via the `extras=[...]` field of your
    /// [`declare_environment!`](../macro.declare_environment.html) or
    /// [`declare_default_environment!`](../macro.declare_default_environment.html).
    ///
    /// This method returns the return value of your closure.
    pub fn with_extras<T, F: FnOnce(&mut E) -> T>(&self, f: F) -> T {
        let mut inner = self.inner.borrow_mut();
        f(&mut *inner)
    }
}

impl<E> Clone for Environment<E> {
    fn clone(&self) -> Environment<E> {
        Environment {
            manager: self.manager.clone(),
            inner: self.inner.clone(),
        }
    }
}

/*
 * Simple handlers
 */

/// A minimalist global handler for "single" globals
///
/// This handler will simply register the global as soon as the registry signals
/// it, and do nothing more.
///
/// It is appropriate for globals that never generate events, like `wl_compositor`
/// or `wl_data_device_manager`.
pub struct SimpleGlobal<I: Interface> {
    global: Option<Attached<I>>,
}

impl<I: Interface> SimpleGlobal<I> {
    /// Create a new handler
    pub fn new() -> SimpleGlobal<I> {
        SimpleGlobal { global: None }
    }
}

impl<I: Interface + Clone + From<Proxy<I>> + AsRef<Proxy<I>>> GlobalHandler<I> for SimpleGlobal<I> {
    fn created(&mut self, registry: Attached<wl_registry::WlRegistry>, id: u32, version: u32) {
        self.global = Some((*registry.bind::<I>(version, id)).clone())
    }
    fn get(&self) -> Option<Attached<I>> {
        self.global.clone()
    }
}

/*
 * environment! macro
 */

/// Macro for creating an environment
///
/// It needs to be used in conjunction with the [`init_environment!`](macro.init_environment.html) macro.
/// This macro declares a type, and the other initializes it at runtime.
///
/// The macro is invoked as such:
///
/// ```no_run
/// # extern crate smithay_client_toolkit as sctk;
/// # use sctk::reexports::client::protocol::{wl_compositor::WlCompositor, wl_shm::WlShm, wl_output::WlOutput};
/// # use sctk::declare_environment;
/// # struct FieldType;
/// declare_environment!(MyEnv,
///     singles = [
///         (compositor, WlCompositor),
///         (shm, WlShm),
///     ],
///     multis = [
///         (outputs, WlOutput),
///     ],
///     extras = [
///         (field_name, FieldType),
///     ]
/// );
/// ```
///
/// This will generate a struct type named `MyEnv`, which is able to manage the `WlCompositor`, `WlShm` and
/// `WlOutput` globals and ignores the rest. For each global, you need to provide a tuple
/// `($name, $type)` where:
///
/// - `$name` is a unique identifier for this global which will be used internally by the generated code
/// - `$type` is the type (implementing the `Interface` trait from `wayland-client`) representing the global
///
/// You can additionnaly insert other values into the environment, by using `extras=[]`. These values can be
/// accessed as field of the struct provided via the `with_extras` method of
/// [`Environment<_>`](environment/struct.Environment.html). The syntax is a tulpe `($name, $type)`:
///
/// - `$name` is the name that will be given to the struct field
/// - `$type` is the type of this field
#[macro_export]
macro_rules! declare_environment {
    ($env_name:ident,
        singles = [$(($sname:ident, $sty:ty)),* $(,)?],
        multis = [$(($mname:ident, $mty:ty)),* $(,)?],
        extras = [$(($ename:ident, $ety:ty)),* $(,)?]
    ) => {
        pub struct $env_name {
            $(
                $sname: Box<$crate::environment::GlobalHandler<$sty>>,
            )*
            $(
                $mname: Box<$crate::environment::MultiGlobalHandler<$mty>>,
            )*
            $(
                $ename: $ety,
            )*
        }

        impl $env_name {
            pub(crate) fn __process_event(
                &mut self,
                event: $crate::reexports::client::GlobalEvent,
                registry: $crate::reexports::client::Attached<$crate::reexports::client::protocol::wl_registry::WlRegistry>
            ) {
                match event {
                    $crate::reexports::client::GlobalEvent::New { id, interface, version } => match &interface[..] {
                        $(
                            <$sty as $crate::reexports::client::Interface>::NAME => self.$sname.created(registry, id, version),
                        )*
                        $(
                            <$mty as $crate::reexports::client::Interface>::NAME => self.$mname.created(registry, id, version),
                        )*
                        _ => { /* ignore unkown globals */ }
                    },
                    $crate::reexports::client::GlobalEvent::Removed { id, interface } => match &interface[..] {
                        $(
                            <$mty as $crate::reexports::client::Interface>::NAME => self.$mname.removed(id),
                        )*
                        _ => { /* ignore unknown globals */ }
                    }
                }
            }
        }

        $(
            impl $crate::environment::GlobalHandler<$sty> for $env_name {
                fn created(&mut self, registry: $crate::reexports::client::Attached<$crate::reexports::client::protocol::wl_registry::WlRegistry>, id: u32, version: u32) {
                    self.$sname.created(registry, id, version)
                }
                fn get(&self) -> Option<$crate::reexports::client::Attached<$sty>> {
                    self.$sname.get()
                }
            }
        )*

        $(
            impl $crate::environment::MultiGlobalHandler<$mty> for $env_name {
                fn created(&mut self, registry: $crate::reexports::client::Attached<$crate::reexports::client::protocol::wl_registry::WlRegistry>, id: u32, version: u32) {
                    self.$mname.created(registry, id, version)
                }
                fn removed(&mut self, id: u32) {
                    self.$mname.removed(id)
                }
                fn get_all(&self) -> Vec<$crate::reexports::client::Attached<$mty>> {
                    self.$mname.get_all()
                }
            }
        )*
    };
}

#[macro_export]
/// Initialize an Environment
///
/// This is the sister macro of [`declare_environment!`](macro.declare_environment.html), which is
/// used to initialize the environment at runtime. It is invoked similarly, but you then provide the
/// values, rather than the types:
///
/// ```no_run
/// # extern crate smithay_client_toolkit as sctk;
/// # use sctk::reexports::client::protocol::{wl_compositor::WlCompositor, wl_shm::WlShm, wl_output::WlOutput};
/// # use sctk::{declare_environment, init_environment};
/// # struct FieldType;
/// # declare_environment!(MyEnv,
/// #     singles = [
/// #         (compositor, WlCompositor),
/// #         (shm, WlShm),
/// #     ],
/// #     multis = [
/// #         (outputs, WlOutput),
/// #     ],
/// #     extras = [
/// #         (field_name, FieldType),
/// #     ]
/// # );
/// # let compositor_handler = sctk::environment::SimpleGlobal::new();
/// # let shm_handler = sctk::environment::SimpleGlobal::new();
/// # let outputs_handler = sctk::output::OutputHandler::new();
/// # let field_value = FieldType;
/// # let display = smithay_client_toolkit::reexports::client::Display::connect_to_env().unwrap();
/// # let mut queue = display.create_event_queue();
/// let env = init_environment!(MyEnv, &display, &mut queue,
///     singles = [
///         (compositor, compositor_handler),
///         (shm, shm_handler),
///     ],
///     multis = [
///         (outputs, outputs_handler),
///     ],
///     extras = [
///         (field_name, field_value),
///     ]
/// );
/// ```
///
/// This macro evaluates to an instance of [`Environment<_>`](environment/struct.Environment.html), from which
/// you'll be able to access the globals in the rest of your app. See its documentation for details.
macro_rules! init_environment {
    ($env_name:ident, $display:expr, $queue:expr,
        singles = [$(($sname:ident, $shandler:expr)),* $(,)?],
        multis = [$(($mname:ident, $mhandler:expr)),* $(,)?],
        extras = [$(($ename:ident, $eval:expr)),* $(,)?]
    ) => {
        {
            use std::{cell::RefCell, rc::Rc};
            use $crate::environment::{Environment, GlobalHandler, MultiGlobalHandler};
            use $crate::reexports::client::{Attached, EventQueue, Interface, GlobalEvent, GlobalManager, Proxy};
            use $crate::reexports::client::protocol::{wl_registry, wl_display};

            let display: &Proxy<wl_display::WlDisplay> = $display;
            let queue: &mut EventQueue = $queue;

            let attached_display = display.clone().attach(queue.get_token());

            let inner = Rc::new(RefCell::new($env_name {
                $(
                    $sname: Box::new($shandler) as Box<_>,
                )*
                $(
                    $mname: Box::new($mhandler) as Box<_>,
                )*
                $(
                    $ename: { $eval }
                )*
            }));

            let my_inner = inner.clone();
            let my_cb = move |event, registry| {
                let mut inner = my_inner.borrow_mut();
                inner.__process_event(event, registry);
            };

            let manager = GlobalManager::new_with_cb(&attached_display, my_cb);

            // a roundtrip to receive the global list
            queue.sync_roundtrip(|evt, obj| {
                panic!(
                    "SCTK: orphan event: {}@{} -> {:?}",
                    evt.interface,
                    obj.as_ref().id(),
                    evt.name
                )
            }).expect("SCTK: Initial roundtrip failed.");
            // a second to let the handlers init their globals
            queue.sync_roundtrip(|evt, obj| {
                panic!(
                    "SCTK: orphan event: {}@{} -> {:?}",
                    evt.interface,
                    obj.as_ref().id(),
                    evt.name
                )
            }).expect("SCTK: initial roundtrip failed.");

            Environment::wrap(manager, inner)
        }
    };
}
