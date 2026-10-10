use super::{ActionEvent, DesktopApp, UiUpdate};
use std::{
    borrow::Cow,
    time::{Duration, Instant},
};
use tao::{
    dpi::LogicalSize,
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoopBuilder},
    platform::run_return::EventLoopExtRunReturn,
    window::{Icon, ResizeDirection, Window, WindowBuilder},
};
use wry::{
    http::{Request, Response},
    NewWindowResponse, WebView, WebViewBuilder,
};

const CSP: &str = "default-src 'none'; style-src 'self' 'unsafe-inline'; script-src 'self'; img-src 'self' data:; font-src 'self'; connect-src 'none'; base-uri 'none'; form-action 'none'; frame-src 'none'; object-src 'none'";
fn local_origin(uri: &str) -> bool {
    uri == "blockhaven://localhost/"
        || uri == "blockhaven://localhost/index.html"
        || uri == "http://blockhaven.localhost/"
        || uri == "http://blockhaven.localhost/index.html"
        || uri == "https://blockhaven.localhost/"
        || uri == "https://blockhaven.localhost/index.html"
}
fn apply(window: &Window, webview: &WebView, mut update: UiUpdate, flow: &mut ControlFlow) {
    if update.close {
        *flow = ControlFlow::Exit;
        return;
    }
    if let Some(command) = update.window.take() {
        window.set_visible(command.visible);
        if command.visible && command.focus {
            window.set_minimized(false);
            window.set_focus();
        }
    }
    update = update.attr(
        ".app-shell",
        "data-maximized",
        Some(window.is_maximized().to_string()),
    );
    if let Ok(json) = serde_json::to_string(&update) {
        let _ = webview.evaluate_script(&format!(
            "window.FastChapp && window.FastChapp.apply({json});"
        ));
    }
}
fn resource(
    app: &'static [super::Asset],
    root: &'static str,
    document: &str,
    request: Request<Vec<u8>>,
) -> Response<Cow<'static, [u8]>> {
    let path = request.uri().path();
    let method = request.method().as_str();
    let mut status = 200;
    let mut mime = "text/plain; charset=utf-8";
    let mut cache = "no-store";
    let mut bytes: Cow<'static, [u8]> = Cow::Borrowed(b"Not found");
    if method != "GET" && method != "HEAD" {
        status = 405;
    } else if path == "/" || path == "/index.html" {
        mime = "text/html; charset=utf-8";
        let mut html = document.to_owned();
        #[cfg(not(feature = "dev"))]
        for asset in app {
            html = html.replace(
                &format!("\"{}\"", asset.path),
                &format!("\"{}\"", asset.hashed_path),
            );
        }
        bytes = Cow::Owned(html.into_bytes());
    } else if let Some(asset) = app.iter().find(|a| a.path == path || a.hashed_path == path) {
        mime = asset.mime;
        if path == asset.hashed_path {
            cache = "public, max-age=31536000, immutable";
        }
        bytes = Cow::Borrowed(asset.bytes);
        #[cfg(feature = "dev")]
        if asset.mime.starts_with("text/css") {
            if let Ok(data) =
                std::fs::read(std::path::Path::new(root).join(asset.path.trim_start_matches('/')))
            {
                bytes = Cow::Owned(data);
                cache = "no-store";
            }
        }
    } else {
        status = 404;
    }
    #[cfg(not(feature = "dev"))]
    let _ = root;
    if method == "HEAD" {
        bytes = Cow::Borrowed(b"");
    }
    Response::builder()
        .status(status)
        .header("Content-Type", mime)
        .header("Cache-Control", cache)
        .header("Content-Security-Policy", CSP)
        .header("X-Content-Type-Options", "nosniff")
        .body(bytes)
        .unwrap()
}
pub fn run(app: DesktopApp) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut event_loop = EventLoopBuilder::<ActionEvent>::with_user_event().build();
    let icon = app
        .icon
        .and_then(|(bytes, w, h)| Icon::from_rgba(bytes.to_vec(), w, h).ok());
    let window = WindowBuilder::new()
        .with_title(&app.title)
        .with_inner_size(LogicalSize::new(1000., 700.))
        .with_min_inner_size(LogicalSize::new(860., 580.))
        .with_decorations(false)
        .with_resizable(true)
        .with_transparent(true)
        .with_window_icon(icon)
        .build(&event_loop)?;
    let document = app.render().map_err(std::io::Error::other)?;
    let proxy = event_loop.create_proxy();
    let assets = app.assets;
    let root = app.resource_root;
    let builder = WebViewBuilder::new()
        .with_transparent(true)
        .with_custom_protocol("blockhaven".into(), move |_, request| {
            resource(assets, root, &document, request)
        })
        .with_url("blockhaven://localhost/index.html")
        .with_initialization_script_for_main_only(include_str!("bridge.js"), true)
        .with_navigation_handler(|uri| local_origin(&uri))
        .with_new_window_req_handler(|_, _| NewWindowResponse::Deny)
        .with_ipc_handler(move |request| {
            if !local_origin(&request.uri().to_string()) {
                return;
            }
            if let Ok(event) = ActionEvent::parse(request.body()) {
                let _ = proxy.send_event(event);
            }
        });
    #[cfg(any(target_os = "windows", target_os = "macos"))]
    let webview = builder.build(&window)?;
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let webview = {
        use tao::platform::unix::WindowExtUnix;
        use wry::WebViewBuilderExtUnix;
        builder.build_gtk(window.default_vbox().ok_or("Missing GTK container")?)?
    };
    #[cfg(feature = "dev")]
    webview.open_devtools();
    let mut next_tick = Instant::now() + Duration::from_millis(250);
    event_loop.run_return(|event, _, flow| {
        *flow = ControlFlow::WaitUntil(next_tick);
        match event {
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => {
                if let Ok(event) = ActionEvent::parse(r#"{"action":"quit"}"#) {
                    let _ = app.dispatch(event);
                }
                *flow = ControlFlow::Exit;
            }
            Event::UserEvent(event) => {
                let update = match event.action.as_str() {
                    "window_close" => app.dispatch(ActionEvent {
                        action: "quit".into(),
                        ..Default::default()
                    }),
                    "window_drag" => {
                        let _ = window.drag_window();
                        Ok(UiUpdate::new())
                    }
                    "window_minimize" => {
                        window.set_minimized(true);
                        Ok(UiUpdate::new())
                    }
                    "window_toggle_maximize" => {
                        window.set_maximized(!window.is_maximized());
                        Ok(UiUpdate::new())
                    }
                    "window_resize" => {
                        let direction = match event.value.as_str() {
                            "north" => Some(ResizeDirection::North),
                            "south" => Some(ResizeDirection::South),
                            "east" => Some(ResizeDirection::East),
                            "west" => Some(ResizeDirection::West),
                            "north_east" => Some(ResizeDirection::NorthEast),
                            "north_west" => Some(ResizeDirection::NorthWest),
                            "south_east" => Some(ResizeDirection::SouthEast),
                            "south_west" => Some(ResizeDirection::SouthWest),
                            _ => None,
                        };
                        if let Some(direction) = direction {
                            let _ = window.drag_resize_window(direction);
                        }
                        Ok(UiUpdate::new())
                    }
                    _ => app.dispatch(event),
                };
                if let Ok(update) = update {
                    apply(&window, &webview, update, flow);
                }
            }
            Event::WindowEvent {
                event: WindowEvent::Resized(_),
                ..
            } => apply(&window, &webview, UiUpdate::new(), flow),
            Event::MainEventsCleared if Instant::now() >= next_tick => {
                next_tick = Instant::now() + Duration::from_millis(250);
                // OS event loop keeps ticking when the window/WebView is hidden.
                apply(&window, &webview, (app.tick)(), flow);
                if *flow != ControlFlow::Exit {
                    *flow = ControlFlow::WaitUntil(next_tick);
                }
            }
            _ => {}
        }
    });
    Ok(())
}
