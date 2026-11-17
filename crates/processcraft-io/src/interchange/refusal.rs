//! Records why an import fell back to preserved source, and which element was being read.
use crate::interchange::import::tree::Elem;
thread_local! {
    /// First (innermost) reason the editable projection was refused, or the element being read.
    pub(super) static REFUSAL: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
    pub(super) static CURRENT: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
}
/// Records why import fell back to preserved source; the first recorded reason wins.
pub(super) fn refused<T>(reason: impl FnOnce() -> String) -> Option<T> {
    let reason = match CURRENT.with(|c| c.borrow().clone()) {
        Some(at) => format!("{} at {at}", reason()),
        None => reason(),
    };
    REFUSAL.with(|r| {
        r.borrow_mut().get_or_insert(reason);
    });
    None
}
/// Marks the element whose qualification is in progress, for refusal messages.
pub(super) fn at(e: &Elem) {
    let label = match e.attrs.get("id") {
        Some(id) => format!("<{} id=\"{id}\">", e.name),
        None => format!("<{}>", e.name),
    };
    CURRENT.with(|c| *c.borrow_mut() = Some(label));
}
pub(super) fn leave() {
    CURRENT.with(|c| c.borrow_mut().take());
}
