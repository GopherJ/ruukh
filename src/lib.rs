#![deny(missing_docs)]
#![cfg_attr(feature = "cargo-clippy", feature(tool_lints))]
#![cfg_attr(feature = "cargo-clippy", warn(clippy::all))]
//! # Ruukh - Introduction
//!
//! Welcome to Ruukh, the frontend web framework.
//!
//! This API reference tries to be both helpful for the users as well as
//! anybody who wants to understand how this framework works. So, if you find
//! anything you do not understand or is wrong here, feel free to open an
//! issue/PR at [github](https://github.com/csharad/ruukh).
//!
//! To create an app, you must first implement a root component which neither
//! accepts props nor accepts events. This component is then mounted on a DOM
//! node like so:
//!
//! # Example
//! ```
//! #![feature(proc_macro_non_items, proc_macro_gen, decl_macro)]
//!
//! use ruukh::prelude::*;
//! use wasm_bindgen::prelude::*;
//!
//! #[component]
//! #[derive(Lifecycle)]
//! struct MyApp;
//!
//! impl Render for MyApp {
//!     fn render(&self) -> Markup<Self> {
//!         html! {
//!             "Hello World!"
//!         }
//!     }
//! }
//!
//! #[wasm_bindgen]
//! pub fn run() {
//!     App::<MyApp>::new().mount("app");
//! }
//! ```
//!
//! Here, "app" is the `id` of an element where you want to mount the App.
//!
//! Note: Docs on macros are located [here](../../ruukh_codegen/index.html).

#[cfg(test)]
use wasm_bindgen_test::*;

#[cfg(test)]
wasm_bindgen_test_configure!(run_in_browser);

use crate::{
    component::{Render, RootParent},
    vdom::vcomponent::{ComponentManager, ComponentWrapper},
};
use std::{cell::RefCell, rc::Rc};
use wasm_bindgen::{prelude::*, JsCast};
use web_sys::{window, Element, MessageChannel, MessagePort};

pub mod component;
mod dom;
pub mod vdom;

/// A VDOM Markup which is generated by using `html!` macro.
pub type Markup<RCTX> = vdom::VNode<RCTX>;

/// Things you'll require to build the next great App. Just glob import the
/// prelude and start building your app.
pub mod prelude {
    pub use crate::component::{Component, Lifecycle, Render, SetState, StateSetter};
    pub use crate::{App, Markup};
    pub use ruukh_codegen::*;
}

/// Things the proc-macro uses without bugging the using to import them.
pub mod reexports {
    pub use fnv::FnvBuildHasher;
    pub use indexmap::IndexMap;
}

/// The main entry point to use your component and run it on the browser.
pub struct App<COMP>
where
    COMP: Render<Props = (), Events = ()>,
{
    manager: ComponentWrapper<COMP, RootParent>,
}

impl<COMP> App<COMP>
where
    COMP: Render<Props = (), Events = ()>,
{
    /// Create a new App with a `Component` struct passed as its type parameter.
    ///
    /// The component that is mounted as an App should not have any props and
    /// events declared onto it.
    ///
    /// # Example
    /// ```
    /// # #![feature(proc_macro_non_items, proc_macro_gen, decl_macro)]
    /// #
    /// # use ruukh::prelude::*;
    /// # use wasm_bindgen::prelude::*;
    /// #
    /// # #[component]
    /// # #[derive(Lifecycle)]
    /// # struct MyApp;
    /// #
    /// # impl Render for MyApp {
    /// #     fn render(&self) -> Markup<Self> {
    /// #         html! {
    /// #             "Hello World!"
    /// #         }
    /// #     }
    /// # }
    /// let my_app = App::<MyApp>::new();
    /// ```
    pub fn new() -> App<COMP> {
        Default::default()
    }

    /// Mounts the app on the given element in the DOM.
    ///
    /// The element may be anything that implements
    /// [AppMount](trait.AppMount.html). You may pass an id of an element
    /// or an element node itself.
    ///
    /// # Example
    /// ```ignore
    /// # #![feature(proc_macro_non_items, proc_macro_gen, decl_macro)]
    /// #
    /// # use ruukh::prelude::*;
    /// # use wasm_bindgen::prelude::*;
    /// #
    /// # #[component]
    /// # #[derive(Lifecycle)]
    /// # struct MyApp;
    /// #
    /// # impl Render for MyApp {
    /// #     fn render(&self) -> Markup<Self> {
    /// #         html! {
    /// #             "Hello World!"
    /// #         }
    /// #     }
    /// # }
    /// App::<MyApp>::new().mount("app");
    /// ```
    pub fn mount(mut self, element: impl AppMount) {
        let parent = element.app_mount();
        let (receiver, sender) = app_message_channel();

        // Every component requires a render context, so provided a void context.
        let root_parent = Rc::new(RefCell::new(()));

        // The first render
        self.manager
            .render_walk(parent.as_ref(), None, root_parent.clone(), sender.clone())
            .unwrap();

        // Rerender when it receives update messages.
        receiver.react_on_message(move || {
            self.manager
                .render_walk(parent.as_ref(), None, root_parent.clone(), sender.clone())
                .unwrap();
        });
    }
}

impl<COMP> Default for App<COMP>
where
    COMP: Render<Props = (), Events = ()>,
{
    /// Create a new App with a component `COMP` that has void props and events.
    fn default() -> Self {
        App {
            manager: ComponentWrapper::new((), ()),
        }
    }
}

/// Create a `MessageChannel` to propagate state change message to the app.
fn app_message_channel() -> (MessageReceiver, MessageSender) {
    let msg_channel = MessageChannel::new().unwrap();
    let is_queued = Rc::new(RefCell::new(false));
    (
        MessageReceiver {
            port: msg_channel.port2(),
            is_queued: is_queued.clone(),
        },
        MessageSender {
            port: msg_channel.port1(),
            is_queued,
        },
    )
}

/// The receiving end of the message port which notifies the app for any state
/// changes.
pub struct MessageReceiver {
    port: MessagePort,
    is_queued: Shared<bool>,
}

impl MessageReceiver {
    /// Invokes the handler, when it receives a message.
    fn react_on_message(self, mut handler: impl FnMut() + 'static) {
        let is_queued = self.is_queued.clone();
        let closure: Closure<dyn FnMut(JsValue)> = Closure::wrap(Box::new(move |_| {
            handler();

            // Unblock the queue.
            *is_queued.borrow_mut() = false;
        }));
        self.port
            .set_onmessage(Some(closure.as_ref().unchecked_ref()));

        // Leak the closure so that the app lives on for 'static lifetimes.
        closure.forget();
    }
}

/// MessageSender is responsible to message the App about state changes.
#[derive(Clone)]
struct MessageSender {
    port: MessagePort,
    is_queued: Shared<bool>,
}

impl MessageSender {
    /// Sends an update message to the App.
    ///
    /// The components need to call this method when they desire the app to
    /// be notified of state changes.
    fn do_react(&self) {
        let is_queued = *self.is_queued.borrow();
        if !is_queued {
            *self.is_queued.borrow_mut() = true;
            // Just send a `null` as we have only a single message to be sent.
            self.port
                .post_message(&JsValue::null())
                .expect("Could not send the message");
        }
    }
}

/// A Shared Value.
type Shared<T> = Rc<RefCell<T>>;

/// Trait to get an element on which the App is going to be mounted.
pub trait AppMount {
    /// Consumes `self` and gets an element from the DOM.
    ///
    /// If the implementation returns an error, panic it instead as it is not
    /// worth it to run the app anymore.
    fn app_mount(self) -> Element;
}

impl<'a> AppMount for &'a str {
    fn app_mount(self) -> Element {
        window()
            .unwrap()
            .document()
            .unwrap()
            .get_element_by_id(self)
            .unwrap_or_else(|| {
                panic!(
                    "Could not find element with id `{}` to mount the App.",
                    self
                )
            })
    }
}

impl AppMount for Element {
    fn app_mount(self) -> Element {
        self
    }
}

impl AppMount for String {
    fn app_mount(self) -> Element {
        self.as_str().app_mount()
    }
}

/// For use in tests.
#[cfg(test)]
fn message_sender() -> MessageSender {
    app_message_channel().1
}
