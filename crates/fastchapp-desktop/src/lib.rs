use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;

#[cfg(feature = "native")]
mod native;
pub type Result<T> = std::result::Result<T, String>;
pub struct Asset {
    pub path: &'static str,
    pub hashed_path: &'static str,
    pub mime: &'static str,
    pub bytes: &'static [u8],
}
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActionEvent {
    pub action: String,
    #[serde(default)]
    pub value: String,
    #[serde(default)]
    pub fields: BTreeMap<String, Vec<String>>,
}
impl ActionEvent {
    pub fn parse(text: &str) -> Result<Self> {
        if text.len() > 64 * 1024 {
            return Err("Action is too large".into());
        }
        let event: Self = serde_json::from_str(text).map_err(|e| e.to_string())?;
        if event.action.is_empty() || event.action.len() > 80 {
            return Err("Invalid action".into());
        }
        Ok(event)
    }
    pub fn field(&self, name: &str) -> &str {
        self.fields
            .get(name)
            .and_then(|v| v.first())
            .map(String::as_str)
            .unwrap_or("")
    }
}
#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct WindowUpdate {
    pub visible: bool,
    pub focus: bool,
}
#[derive(Debug, Default, Serialize)]
pub struct UiUpdate {
    pub patches: Vec<Value>,
    pub close: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub window: Option<WindowUpdate>,
}
impl UiUpdate {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn html(mut self, selector: &str, content: impl Into<String>) -> Self {
        self.patches
            .push(json!({"op":"html","selector":selector,"value":content.into()}));
        self
    }
    pub fn text(mut self, selector: &str, content: impl Into<String>) -> Self {
        self.patches
            .push(json!({"op":"text","selector":selector,"value":content.into()}));
        self
    }
    pub fn value(mut self, selector: &str, value: impl Into<String>) -> Self {
        self.patches
            .push(json!({"op":"value","selector":selector,"value":value.into()}));
        self
    }
    pub fn attr(mut self, selector: &str, name: &str, value: Option<String>) -> Self {
        self.patches
            .push(json!({"op":"attr","selector":selector,"name":name,"value":value}));
        self
    }
    pub fn window(mut self, visible: bool, focus: bool) -> Self {
        self.window = Some(WindowUpdate { visible, focus });
        self
    }
    pub fn close(mut self) -> Self {
        self.close = true;
        self
    }
}
pub struct DesktopApp {
    pub assets: &'static [Asset],
    pub resource_root: &'static str,
    pub title: String,
    pub icon: Option<(&'static [u8], u32, u32)>,
    pub view: Box<dyn Fn() -> Result<String>>,
    pub handler: Box<dyn Fn(ActionEvent) -> Result<UiUpdate>>,
    pub tick: Box<dyn Fn() -> UiUpdate>,
}
impl DesktopApp {
    pub fn render(&self) -> Result<String> {
        (self.view)()
    }
    pub fn dispatch(&self, event: ActionEvent) -> Result<UiUpdate> {
        (self.handler)(event)
    }
    #[cfg(feature = "native")]
    pub fn run(self) -> std::result::Result<(), Box<dyn std::error::Error>> {
        native::run(self)
    }
}
pub fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn actions_are_bounded_and_typed() {
        assert!(ActionEvent::parse(r#"{"action":"play","fields":{"x":["yes"]}}"#).is_ok());
        assert!(ActionEvent::parse(r#"{"action":"play","fields":{"x":"yes"}}"#).is_err());
        assert!(ActionEvent::parse(&"x".repeat(70000)).is_err());
    }
    #[test]
    fn visibility_is_native_metadata() {
        let update = UiUpdate::new().window(true, true).html("body", "hello");
        assert_eq!(
            update.window,
            Some(WindowUpdate {
                visible: true,
                focus: true
            })
        );
        assert_eq!(update.patches.len(), 1);
    }
}
