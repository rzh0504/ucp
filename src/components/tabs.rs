use dioxus::prelude::*;
use dioxus_primitives::tabs as primitive;

#[derive(Props, Clone, PartialEq)]
pub struct TabsProps {
    #[props(default)]
    pub class: String,
    pub value: ReadSignal<Option<String>>,
    #[props(default)]
    pub default_value: String,
    #[props(default)]
    pub on_value_change: Callback<String>,
    #[props(default)]
    pub disabled: ReadSignal<bool>,
    #[props(default)]
    pub horizontal: ReadSignal<bool>,
    #[props(default = ReadSignal::new(Signal::new(true)))]
    pub roving_loop: ReadSignal<bool>,
    pub children: Element,
}

#[component]
pub fn Tabs(props: TabsProps) -> Element {
    rsx! {
        primitive::Tabs {
            class: "dx-tabs {props.class}",
            "data-variant": "default",
            value: props.value,
            default_value: props.default_value,
            on_value_change: props.on_value_change,
            disabled: props.disabled,
            horizontal: props.horizontal,
            roving_loop: props.roving_loop,
            {props.children}
        }
    }
}

#[derive(Props, Clone, PartialEq)]
pub struct TabListProps {
    #[props(default)]
    pub class: String,
    #[props(default)]
    pub aria_label: Option<String>,
    pub children: Element,
}

#[component]
pub fn TabList(props: TabListProps) -> Element {
    rsx! {
        primitive::TabList {
            class: "dx-tabs-list {props.class}",
            aria_label: props.aria_label,
            {props.children}
        }
    }
}

#[derive(Props, Clone, PartialEq)]
pub struct TabTriggerProps {
    #[props(default)]
    pub class: String,
    pub value: String,
    pub index: ReadSignal<usize>,
    pub children: Element,
}

#[component]
pub fn TabTrigger(props: TabTriggerProps) -> Element {
    rsx! {
        primitive::TabTrigger {
            class: Some(format!("dx-tabs-trigger {}", props.class)),
            value: props.value,
            index: props.index,
            {props.children}
        }
    }
}
