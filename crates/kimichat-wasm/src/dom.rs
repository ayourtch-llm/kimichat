use wasm_bindgen::{JsValue, JsCast};
use web_sys::{Document, Element, HtmlElement, HtmlInputElement, HtmlTextAreaElement};

/// Get element by ID
pub fn get_element_by_id(document: &Document, id: &str) -> Result<Element, JsValue> {
    document
        .get_element_by_id(id)
        .ok_or_else(|| JsValue::from_str(&format!("Element not found: {}", id)))
}

/// Get HTML element by ID
pub fn get_html_element_by_id(document: &Document, id: &str) -> Result<HtmlElement, JsValue> {
    let element = get_element_by_id(document, id)?;
    element
        .dyn_into::<HtmlElement>()
        .map_err(|_| JsValue::from_str(&format!("Element is not HtmlElement: {}", id)))
}

/// Get input element by ID
pub fn get_input_by_id(document: &Document, id: &str) -> Result<HtmlInputElement, JsValue> {
    let element = get_element_by_id(document, id)?;
    element
        .dyn_into::<HtmlInputElement>()
        .map_err(|_| JsValue::from_str(&format!("Element is not HtmlInputElement: {}", id)))
}

/// Get textarea element by ID
pub fn get_textarea_by_id(document: &Document, id: &str) -> Result<HtmlTextAreaElement, JsValue> {
    let element = get_element_by_id(document, id)?;
    element
        .dyn_into::<HtmlTextAreaElement>()
        .map_err(|_| JsValue::from_str(&format!("Element is not HtmlTextAreaElement: {}", id)))
}

/// Create element with class
pub fn create_element_with_class(
    document: &Document,
    tag: &str,
    class: &str,
) -> Result<Element, JsValue> {
    let element = document.create_element(tag)?;
    element.set_class_name(class);
    Ok(element)
}

/// Set inner HTML safely (caller should ensure content is safe)
pub fn set_inner_html(element: &Element, html: &str) {
    element.set_inner_html(html);
}

/// Set text content
pub fn set_text_content(element: &Element, text: &str) {
    element.set_text_content(Some(text));
}

/// Add event listener to element
pub fn add_click_listener<F>(element: &Element, callback: F) -> Result<(), JsValue>
where
    F: FnMut() + 'static,
{
    use wasm_bindgen::closure::Closure;
    use wasm_bindgen::JsCast;

    let closure = Closure::wrap(Box::new(callback) as Box<dyn FnMut()>);
    element
        .add_event_listener_with_callback("click", closure.as_ref().unchecked_ref())?;
    closure.forget(); // Keep the closure alive
    Ok(())
}

/// Show element
pub fn show_element(element: &HtmlElement) {
    let _ = element.style().set_property("display", "block");
}

/// Hide element
pub fn hide_element(element: &HtmlElement) {
    let _ = element.style().set_property("display", "none");
}

/// Clear element content
pub fn clear_element(element: &Element) {
    element.set_inner_html("");
}

/// Scroll element to bottom
pub fn scroll_to_bottom(element: &Element) {
    if let Ok(html_element) = element.clone().dyn_into::<HtmlElement>() {
        html_element.set_scroll_top(html_element.scroll_height());
    }
}
