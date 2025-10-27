use std::{cell::RefCell, rc::Rc};

use gloo_net::http::Request;
use ratatui::{
    text::Line,
    widgets::{Borders, Wrap},
};
use ratzilla::ratatui::{
    layout::{Alignment, Constraint, Layout},
    style::{Color, Stylize},
    text::Text,
    widgets::{Block, BorderType, Paragraph},
    Frame, Terminal,
};

use ratzilla::{
    event::{KeyCode, KeyEvent},
    DomBackend, WebRenderer,
};
use wasm_bindgen::{prelude::wasm_bindgen, JsError};

#[wasm_bindgen(start)]
fn main() -> Result<(), JsError> {
    // Optional: readable panic messages in the browser console
    console_error_panic_hook::set_once();

    let backend = DomBackend::new()?;
    let terminal = Terminal::new(backend)?;

    let state = Rc::new(App::new());

    let event_state = Rc::clone(&state);
    terminal.on_key_event(move |key_event| {
        let event_state = event_state.clone();
        wasm_bindgen_futures::spawn_local(async move {
            event_state.handle_events(key_event).await;
        });
    });

    let render_state = Rc::clone(&state);
    terminal.draw_web(move |frame| {
        render_state.render(frame);
    });

    Ok(())
}

#[derive(Clone, Debug)]
pub struct FilePreview {
    path: String,
    preview: String,
}

#[derive(Default)]
struct App {
    counter: RefCell<u8>,
    loaded_text: RefCell<Option<String>>,
    vfs: RefCell<Option<InMemoryVfs>>,
    previews: RefCell<Vec<FilePreview>>,
    status: RefCell<String>,
}

impl App {
    fn new() -> Self {
        Self {
            status: RefCell::new("Press U to upload a crate".into()),
            previews: RefCell::new(Vec::new()),
            vfs: RefCell::new(None),
            ..Default::default()
        }
    }
    fn rebuild_previews(&self) {
        let mut previews = self.previews.borrow_mut();
        previews.clear();
        if let Some(vfs) = &*self.vfs.borrow() {
            for path in vfs.list() {
                let bytes = vfs.read(&path).unwrap_or_default();
                let s = String::from_utf8_lossy(&bytes).replace('\n', " ");
                let short = s.chars().take(30).collect::<String>();
                previews.push(FilePreview {
                    path,
                    preview: short,
                });
            }
        }
        if self.vfs.borrow().is_some() {
            self.status.replace(format!(
                "Loaded {} files. Press E to export.",
                self.previews.borrow().len()
            ));
        }
    }
    fn render(&self, frame: &mut Frame) {
        let chunks = Layout::vertical([
            Constraint::Length(10),
            Constraint::Length(10),
            Constraint::Min(0),
        ])
        .split(frame.area());

        let counter = self.counter.borrow();
        let paragraph = generate_paragraph(counter);

        frame.render_widget(paragraph, chunks[0]);

        let loaded_text = self.loaded_text.borrow();
        let loaded_paragraph = generate_loaded_text(loaded_text);
        frame.render_widget(loaded_paragraph, chunks[1]);

        let loaded_files = generate_file_previews(self);
        frame.render_widget(loaded_files, chunks[2]);
    }

    async fn handle_events(&self, key_event: KeyEvent) {
        match key_event.code {
            KeyCode::Left => {
                let mut counter = self.counter.borrow_mut();
                *counter = counter.saturating_sub(1);
            }
            KeyCode::Right => {
                let mut counter = self.counter.borrow_mut();
                *counter = counter.saturating_add(1);
            }
            KeyCode::Char('l') if self.loaded_text.borrow().is_none() => {
                let text = load_text("assets/sample.txt").await;
                self.loaded_text.replace(text);
            }
            KeyCode::Char('u') => match mount_picked_crate().await {
                Ok(vfs) => {
                    self.vfs.replace(Some(vfs));
                    self.rebuild_previews();
                }
                Err(e) => {
                    self.status
                        .replace(format!("Failed to load crate: {:?}", e));
                }
            },
            KeyCode::Char('e') => {
                if let Some(vfs) = &*self.vfs.borrow() {
                    if let Err(e) = export_as_zip(vfs) {
                        self.status.replace(format!("Export failed: {:?}", e));
                    }
                }
            }
            _ => {}
        }
    }
}

fn generate_paragraph(counter: std::cell::Ref<'_, u8>) -> Paragraph<'_> {
    let block = Block::bordered()
        .title("generic-rat")
        .title_alignment(Alignment::Center)
        .border_type(BorderType::Rounded);

    let text = format!(
        "This is a Ratzilla template.\n\
             Press left and right to increment and decrement the counter respectively.\n\
             Counter: {counter}",
    );

    let paragraph = Paragraph::new(text)
        .block(block)
        .fg(Color::White)
        .bg(Color::Black)
        .centered();
    paragraph
}

fn generate_loaded_text(text: std::cell::Ref<'_, Option<String>>) -> Paragraph<'_> {
    let block = Block::bordered()
        .title("loaded-text")
        .title_alignment(Alignment::Center)
        .border_type(BorderType::Rounded);

    let texty_text = match text.clone() {
        Some(t) => Text::from(t),
        None => Text::default(),
    };
    let paragraph = Paragraph::new(texty_text)
        .block(block)
        .fg(Color::White)
        .bg(Color::Black)
        .centered();
    paragraph
}

fn generate_file_previews<'a>(app: &'a App) -> Paragraph<'a> {
    let mut lines: Vec<Line> = Vec::with_capacity(app.previews.borrow().len() + 2);

    lines.push(Line::from(app.status.borrow().clone()));
    lines.push(Line::from(" "));

    for fp in &*app.previews.borrow() {
        let line = format!("{}: {}", fp.path, fp.preview);
        lines.push(Line::from(line));
    }

    Paragraph::new(lines)
        .block(
            Block::default()
                .title("Uploaded Crate")
                .borders(Borders::ALL),
        )
        .wrap(Wrap { trim: true })
}

async fn load_text(file_path: &'static str) -> Option<String> {
    let text = match Request::get(file_path).send().await {
        Ok(resp) => resp
            .text()
            .await
            .unwrap_or_else(|_| "<failed to read text>".into()),
        Err(err) => format!("<fetch failed: {err}>"),
    };
    Some(text)
}

pub trait Vfs {
    fn list(&self) -> Vec<String>; // relative paths
    fn read(&self, path: &str) -> Option<Vec<u8>>; // returns file bytes
    fn write(&mut self, path: &str, data: Vec<u8>); // in-memory edits
}

pub struct InMemoryVfs {
    files: std::collections::BTreeMap<String, Vec<u8>>,
}

impl Vfs for InMemoryVfs {
    fn list(&self) -> Vec<String> {
        self.files.keys().cloned().collect()
    }
    fn read(&self, path: &str) -> Option<Vec<u8>> {
        self.files.get(path).cloned()
    }
    fn write(&mut self, path: &str, data: Vec<u8>) {
        self.files.insert(path.to_string(), data);
    }
}

use js_sys::{Array, Uint8Array};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = window, js_name = pickRustCrate)]
    async fn pick_rust_crate() -> JsValue;

    #[wasm_bindgen(js_namespace = window, js_name = pickRustCrateFallback)]
    async fn pick_rust_crate_fallback() -> JsValue;

    #[wasm_bindgen(js_namespace = window, js_name = downloadAsZip)]
    fn download_as_zip(files: JsValue);
}

#[derive(Clone)]
pub struct FileEntry {
    pub path: String,
    pub bytes: Vec<u8>,
}

async fn gather_files() -> Result<Vec<FileEntry>, JsValue> {
    let has_fs_api = web_sys::window()
        .and_then(|w| js_sys::Reflect::get(&w, &JsValue::from_str("showDirectoryPicker")).ok())
        .map(|v| v.is_function())
        .unwrap_or(false);

    let js = if has_fs_api {
        pick_rust_crate().await
    } else {
        pick_rust_crate_fallback().await
    };

    let arr: Array = js.dyn_into()?;
    let mut out = Vec::with_capacity(arr.length() as usize);
    for v in arr.iter() {
        let path = js_sys::Reflect::get(&v, &JsValue::from_str("path"))?
            .as_string()
            .unwrap_or_default();
        let bytes = js_sys::Reflect::get(&v, &JsValue::from_str("bytes"))?;
        let bytes = Uint8Array::new(&bytes).to_vec();
        out.push(FileEntry { path, bytes });
    }
    Ok(out)
}

pub async fn mount_picked_crate() -> Result<InMemoryVfs, JsValue> {
    let files = gather_files().await?;
    let mut map = std::collections::BTreeMap::new();
    for f in files {
        map.insert(f.path, f.bytes);
    }
    Ok(InMemoryVfs { files: map })
}

pub fn export_as_zip(vfs: &impl Vfs) -> Result<(), JsValue> {
    let files = Array::new();

    for p in vfs.list() {
        let bytes = vfs.read(&p).unwrap_or_default();

        let rec = js_sys::Object::new();
        // path
        js_sys::Reflect::set(&rec, &JsValue::from_str("path"), &JsValue::from_str(&p))?;
        // bytes (Uint8Array)
        let u8 = Uint8Array::from(bytes.as_slice());
        js_sys::Reflect::set(&rec, &JsValue::from_str("bytes"), &u8.into())?;

        // Push JsValue into the Array explicitly:
        files.push(&JsValue::from(rec));
    }

    download_as_zip(files.into());
    Ok(())
}
