//! Component representation in a VDOM.

use component::{ComponentStatus, RenderableComponent};
use dom::DOMPatch;
use std::any::Any;
use std::cell::RefCell;
use std::fmt::{self, Display, Formatter};
use std::rc::Rc;
use wasm_bindgen::prelude::JsValue;
use web_api::*;
use {KeyedVNodes, Shared, VNode};

/// The representation of a component in a Virtual DOM.
pub struct VComponent(Box<ComponentManager>);

impl VComponent {
    #[allow(missing_docs)]
    pub fn new<COMP: RenderableComponent + 'static>(props: COMP::Props) -> VComponent {
        VComponent(Box::new(ComponentWrapper::<COMP>::new(props)))
    }
}

struct ComponentWrapper<COMP: RenderableComponent + 'static> {
    component: Option<Shared<COMP>>,
    props: Option<COMP::Props>,
    cached_render: Option<KeyedVNodes>,
}

impl<COMP: RenderableComponent + 'static> ComponentWrapper<COMP> {
    fn new(props: COMP::Props) -> ComponentWrapper<COMP> {
        ComponentWrapper {
            component: None,
            props: Some(props),
            cached_render: None,
        }
    }

    fn try_cast(
        other: Box<ComponentManager>,
    ) -> Result<ComponentWrapper<COMP>, Box<ComponentManager>> {
        let mut same_type = false;
        {
            let any = &other as &Any;
            if any.is::<ComponentWrapper<COMP>>() {
                same_type = true;
            }
        }

        if same_type {
            let boxed = other.into_any();
            Ok(*boxed
                .downcast::<ComponentWrapper<COMP>>()
                .expect("Impossible! The type cannot be different."))
        } else {
            Err(other)
        }
    }
}

impl DOMPatch for VComponent {
    type Node = Node;

    fn render_walk(&mut self, parent: Self::Node, next: Option<Self::Node>) -> Result<(), JsValue> {
        self.0.render_walk(parent, next)
    }

    fn patch(
        &mut self,
        old: Option<Self>,
        parent: Self::Node,
        next: Option<Self::Node>,
    ) -> Result<(), JsValue> {
        self.0.patch(old.map(|old| old.0), parent, next)
    }

    fn remove(mut self, parent: Self::Node) -> Result<(), JsValue> {
        self.0.remove(parent)
    }

    fn node(&self) -> Option<Node> {
        self.0.node()
    }
}

trait ComponentManager: Downcast + Display {
    fn render_walk(&mut self, parent: Node, next: Option<Node>) -> Result<(), JsValue>;

    fn patch(
        &mut self,
        old: Option<Box<ComponentManager>>,
        parent: Node,
        next: Option<Node>,
    ) -> Result<(), JsValue>;

    fn remove(&mut self, parent: Node) -> Result<(), JsValue>;

    fn node(&self) -> Option<Node>;
}

impl<COMP: RenderableComponent + 'static> ComponentManager for ComponentWrapper<COMP> {
    fn render_walk(&mut self, parent: Node, next: Option<Node>) -> Result<(), JsValue> {
        if self.component.is_none() {
            let props = self.props.take().unwrap();
            let instance = COMP::init(props, ComponentStatus::new(COMP::State::default()));
            instance.created();
            let mut initial_render = instance.render();
            initial_render.patch(None, parent.clone(), next.clone())?;
            self.component = Some(Rc::new(RefCell::new(instance)));
            self.cached_render = Some(initial_render);
        } else {
            let comp = self.component.as_ref().unwrap();

            let state_changed = if comp.borrow_mut().is_state_dirty() {
                comp.borrow_mut().refresh_state()
            } else {
                false
            };

            if state_changed || comp.borrow_mut().is_props_dirty() {
                let mut rerender = comp.borrow().render();
                let cached_render = self.cached_render.take();
                rerender.patch(cached_render, parent.clone(), next.clone())?;
                self.cached_render = Some(rerender);
            }
        }
        self.render_walk(parent, next)
    }

    fn patch(
        &mut self,
        old: Option<Box<ComponentManager>>,
        parent: Node,
        next: Option<Node>,
    ) -> Result<(), JsValue> {
        if let Some(old) = old {
            match Self::try_cast(old) {
                Ok(same) => {
                    let comp = same.component.unwrap();

                    // Reuse the older component by passing in the newer props.
                    if let Some(old_props) = comp.borrow_mut().update(self.props.take().unwrap()) {
                        comp.borrow().updated(old_props);
                    }
                    self.component = Some(comp);

                    // Reuse the cached render too to do patches on.
                    self.cached_render = same.cached_render;
                }
                Err(mut not_same) => {
                    // The component is not the same, remove it from the DOM tree.
                    not_same.remove(parent.clone())?;
                }
            }
        }
        self.render_walk(parent, next)
    }

    fn remove(&mut self, parent: Node) -> Result<(), JsValue> {
        if let Some(cached_render) = self.cached_render.take() {
            cached_render.remove(parent)?;
            let comp = self.component.as_ref().unwrap();
            comp.borrow().destroyed();
        }
        Ok(())
    }

    fn node(&self) -> Option<Node> {
        self.cached_render.as_ref().and_then(|inner| inner.node())
    }
}

impl From<VComponent> for VNode {
    fn from(comp: VComponent) -> VNode {
        VNode::Component(comp)
    }
}

trait Downcast: Any {
    fn into_any(self: Box<Self>) -> Box<Any>;
}

impl<T: Any> Downcast for T {
    fn into_any(self: Box<Self>) -> Box<Any> {
        self
    }
}

impl Display for VComponent {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl<COMP: RenderableComponent + 'static> Display for ComponentWrapper<COMP> {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(
            f,
            "{}",
            self.cached_render
                .as_ref()
                .expect("Render the component first.")
        )
    }
}

#[cfg(test)]
pub mod wasm_test {
    use dom::*;
    use prelude::*;
    use wasm_bindgen_test::*;
    use web_api::*;

    #[derive(Debug)]
    struct Button {
        disabled: bool,
        __status: ComponentStatus<()>,
    }

    struct ButtonProps {
        disabled: bool,
    }

    impl Lifecycle for Button {}

    impl Component for Button {
        type Props = ButtonProps;
        type State = ();

        fn init(props: Self::Props, status: ComponentStatus<Self::State>) -> Self {
            Button {
                disabled: props.disabled,
                __status: status,
            }
        }

        fn props(self) -> Self::Props {
            ButtonProps {
                disabled: self.disabled,
            }
        }

        fn status(&self) -> ComponentStatus<Self::State> {
            self.__status.clone()
        }

        fn refresh_state(&mut self) {}

        fn is_dirty(&self) -> bool {
            false
        }

        fn mark_clean(&mut self) {}
    }

    impl Render for Button {
        fn render(&self) -> KeyedVNodes {
            KeyedVNodes::unkeyed(VElement::new(
                "button",
                vec![Attribute::new("disabled", self.disabled.to_string())],
                KeyedVNodes::unkeyed(VText::text("Click")),
            ))
        }
    }

    fn container() -> Element {
        html_document.create_element("div").unwrap()
    }

    #[wasm_bindgen_test]
    fn should_patch_container_with_component() {
        let mut vcomp = VComponent::new::<Button>(ButtonProps { disabled: false });
        let div = container();
        vcomp
            .patch(None, div.clone().into(), None)
            .expect("To patch div");

        assert_eq!(
            div.inner_html(),
            r#"<button disabled="false">Click</button>"#
        );
    }

    #[wasm_bindgen_test]
    fn should_patch_container_with_component_update() {
        let mut vcomp = VComponent::new::<Button>(ButtonProps { disabled: false });
        let div = container();
        vcomp
            .patch(None, div.clone().into(), None)
            .expect("To patch div");

        assert_eq!(
            div.inner_html(),
            r#"<button disabled="false">Click</button>"#
        );

        let mut patched = VComponent::new::<Button>(ButtonProps { disabled: true });
        patched
            .patch(Some(vcomp), div.clone().into(), None)
            .expect("To patch div");

        assert_eq!(
            div.inner_html(),
            r#"<button disabled="true">Click</button>"#
        );
    }
}
