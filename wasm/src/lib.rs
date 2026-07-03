use wasm_bindgen::{closure::Closure, prelude::*, JsCast};
use web_sys::{window, Event, HtmlTextAreaElement, MessageEvent, WebSocket};

#[wasm_bindgen(start)]
pub fn start() -> Result<(), JsValue> {
    let w = window().ok_or("no window")?;
    let d = w.document().ok_or("no document")?;
    let t = d
        .get_element_by_id("t")
        .ok_or("missing textarea")?
        .dyn_into::<HtmlTextAreaElement>()?;

    let proto = if w.location().protocol()? == "https:" { "wss" } else { "ws" };
    let ws = WebSocket::new(&format!("{}://{}/ws", proto, w.location().host()?))?;

    let ws_send = ws.clone();
    let t_send = t.clone();
    let oninput = Closure::<dyn FnMut(Event)>::new(move |_| {
        let _ = ws_send.send_with_str(&t_send.value());
    });
    t.set_oninput(Some(oninput.as_ref().unchecked_ref()));
    oninput.forget();

    let t_recv = t.clone();
    let onmsg = Closure::<dyn FnMut(Event)>::new(move |e: Event| {
        if let Ok(me) = e.dyn_into::<MessageEvent>() {
            if let Some(s) = me.data().as_string() {
                t_recv.set_value(&s);
            }
        }
    });
    ws.set_onmessage(Some(onmsg.as_ref().unchecked_ref()));
    onmsg.forget();

    Ok(())
}
