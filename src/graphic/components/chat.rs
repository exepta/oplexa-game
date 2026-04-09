use bevy_inspector_egui::bevy_egui::{EguiContexts, egui};
use crate::core::chat::ChatLog;
use crate::core::commands::default_chat_command_registry;
use crate::core::events::ui_events::ChatSubmitRequest;

const CHAT_IDLE_BEFORE_FADE_SECS: f32 = 5.0;
const CHAT_FADE_DURATION_SECS: f32 = 3.0;
const CHAT_PANEL_WIDTH: f32 = 750.0;
const CHAT_PANEL_MAX_HEIGHT: f32 = 220.0;
const CHAT_PANEL_BOTTOM_OFFSET: f32 = 100.0;
const CHAT_INPUT_HEIGHT: f32 = 18.0;
const CHAT_FONT_SIZE: f32 = 13.0;
const CHAT_RENDERED_LINES_CLOSED: usize = 10;

/// Represents chat ui runtime state used by the `graphic::components::chat` module.
#[derive(Resource, Debug, Default)]
struct ChatUiState {
    open: bool,
    input: String,
    idle_secs: f32,
    alpha: f32,
    last_seen_line_count: usize,
    focus_input_next_frame: bool,
    suggestion: Option<String>,
}

/// Updates chat input/open state and fade timers for the `graphic::components::chat` module.
fn update_chat_ui_state(
    time: Res<Time>,
    keyboard: Res<ButtonInput<KeyCode>>,
    global_config: Res<GlobalConfig>,
    multiplayer_connection: Res<MultiplayerConnectionState>,
    mut chat_log: ResMut<ChatLog>,
    mut chat_ui: ResMut<ChatUiState>,
    mut ui_interaction: ResMut<UiInteractionState>,
    mut cursor_q: Query<&mut CursorOptions, With<PrimaryWindow>>,
    mut submit: MessageWriter<ChatSubmitRequest>,
) {
    let mut consumed_enter_this_frame = false;
    let mut consumed_close_this_frame = false;

    chat_log.set_max_lines(global_config.interface.chat_max_space);

    if chat_log.lines().len() != chat_ui.last_seen_line_count {
        chat_ui.last_seen_line_count = chat_log.lines().len();
        if chat_ui.last_seen_line_count > 0 {
            chat_ui.idle_secs = 0.0;
            chat_ui.alpha = 1.0;
        }
    }

    let open_chat_key = convert(global_config.input.open_chat.as_str()).unwrap_or(KeyCode::KeyC);
    let close_key = convert(global_config.input.ui_close_back.as_str()).unwrap_or(KeyCode::Escape);

    if ui_interaction.menu_open || ui_interaction.inventory_open {
        chat_ui.open = false;
    }

    if !chat_ui.open
        && !ui_interaction.menu_open
        && !ui_interaction.inventory_open
        && keyboard.just_pressed(open_chat_key)
    {
        chat_ui.open = true;
        chat_ui.idle_secs = 0.0;
        chat_ui.alpha = 1.0;
        chat_ui.focus_input_next_frame = true;
    }

    if chat_ui.open && keyboard.just_pressed(close_key) {
        chat_ui.open = false;
        consumed_close_this_frame = true;
    }

    if chat_ui.open {
        if keyboard.just_pressed(KeyCode::Tab)
            && let Some(suggestion) = chat_ui.suggestion.clone()
        {
            chat_ui.input = suggestion;
        }

        if keyboard.just_pressed(KeyCode::Enter) || keyboard.just_pressed(KeyCode::NumpadEnter) {
            let text = chat_ui.input.trim();
            if !text.is_empty() {
                submit.write(ChatSubmitRequest {
                    text: text.to_string(),
                });
            }
            chat_ui.input.clear();
            chat_ui.open = false;
            consumed_enter_this_frame = true;
        }

        chat_ui.idle_secs = 0.0;
        chat_ui.alpha = 1.0;
    } else if chat_ui.last_seen_line_count == 0 {
        chat_ui.alpha = 0.0;
    } else {
        chat_ui.idle_secs += time.delta_secs();
        if chat_ui.idle_secs <= CHAT_IDLE_BEFORE_FADE_SECS {
            chat_ui.alpha = 1.0;
        } else {
            let fade_t =
                (chat_ui.idle_secs - CHAT_IDLE_BEFORE_FADE_SECS) / CHAT_FADE_DURATION_SECS;
            chat_ui.alpha = (1.0 - fade_t).clamp(0.0, 1.0);
        }
    }

    chat_ui.suggestion = if chat_ui.open {
        resolve_chat_suggestion(
            chat_ui.input.as_str(),
            multiplayer_connection.known_player_names.as_slice(),
        )
    } else {
        None
    };

    ui_interaction.chat_open = chat_ui.open || consumed_enter_this_frame || consumed_close_this_frame;
    if chat_ui.open
        && let Ok(mut cursor) = cursor_q.single_mut()
    {
        cursor.grab_mode = CursorGrabMode::None;
        cursor.visible = true;
    }
}

fn resolve_chat_suggestion(input: &str, known_player_names: &[String]) -> Option<String> {
    if input.trim_start().starts_with('/') {
        let suggestions = default_chat_command_registry().autocomplete(input);
        return suggestions
            .into_iter()
            .find(|candidate| candidate != input.trim());
    }

    autocomplete_mention(input, known_player_names)
}

fn autocomplete_mention(input: &str, known_player_names: &[String]) -> Option<String> {
    if known_player_names.is_empty() {
        return None;
    }

    let at_index = input.rfind('@')?;
    let suffix = &input[at_index + 1..];

    let mut tail = suffix;
    let mut has_space_after_at = false;
    if tail.starts_with(' ') {
        has_space_after_at = true;
        tail = tail.trim_start_matches(' ');
    }

    let mut braced = false;
    if tail.starts_with('{') {
        braced = true;
        tail = &tail[1..];
    }

    if tail.contains('}') || tail.contains(char::is_whitespace) {
        return None;
    }

    let prefix = tail.to_ascii_lowercase();
    let mut candidates = known_player_names.to_vec();
    candidates.sort_by_key(|name| name.to_ascii_lowercase());

    let best = candidates
        .into_iter()
        .find(|name| name.to_ascii_lowercase().starts_with(prefix.as_str()))?;

    let replacement = if braced {
        format!("@{}{{{}}}", if has_space_after_at { " " } else { "" }, best)
    } else {
        format!("@{}{}", if has_space_after_at { " " } else { "" }, best)
    };
    Some(format!("{}{}", &input[..at_index], replacement))
}

/// Renders the chat ui overlay for the `graphic::components::chat` module.
fn render_chat_overlay(
    mut egui_contexts: EguiContexts,
    chat_log: Res<ChatLog>,
    mut chat_ui: ResMut<ChatUiState>,
    mut submit: MessageWriter<ChatSubmitRequest>,
) {
    if !chat_ui.open && chat_ui.alpha <= 0.01 {
        return;
    }

    let Ok(ctx) = egui_contexts.ctx_mut() else {
        return;
    };

    let alpha = chat_ui.alpha.clamp(0.0, 1.0);
    let frame_fill = egui::Color32::from_black_alpha((170.0 * alpha) as u8);
    let text_color = egui::Color32::from_white_alpha((255.0 * alpha) as u8);
    let mention_color = egui::Color32::from_white_alpha((255.0 * alpha) as u8);
    let hint_color = egui::Color32::from_white_alpha((255.0 * alpha) as u8);
    let location_color = egui::Color32::from_white_alpha((255.0 * alpha) as u8);
    let accent_color = egui::Color32::from_rgb(0x40, 0xc2, 0x99);

    egui::Area::new("oplexa-chat-overlay".into())
        .order(egui::Order::Foreground)
        .anchor(egui::Align2::LEFT_BOTTOM, [16.0, -CHAT_PANEL_BOTTOM_OFFSET])
        .interactable(chat_ui.open)
        .show(ctx, |ui| {
            egui::Frame::default()
                .fill(frame_fill)
                .stroke(egui::Stroke::new(
                    1.0,
                    egui::Color32::from_white_alpha((90.0 * alpha) as u8),
                ))
                .corner_radius(egui::CornerRadius::same(6))
                .inner_margin(egui::Margin::same(8))
                .show(ui, |ui| {
                    ui.set_width(CHAT_PANEL_WIDTH);
                    let mut style = ui.style().as_ref().clone();
                    style.text_styles.insert(
                        egui::TextStyle::Monospace,
                        egui::FontId::monospace(CHAT_FONT_SIZE),
                    );
                    style.text_styles.insert(
                        egui::TextStyle::Body,
                        egui::FontId::proportional(CHAT_FONT_SIZE),
                    );
                    style.visuals.override_text_color = Some(text_color);
                    ui.set_style(style);

                    let lines = chat_log.lines();
                    let start = if chat_ui.open {
                        0
                    } else {
                        lines.len().saturating_sub(CHAT_RENDERED_LINES_CLOSED)
                    };
                    egui::ScrollArea::vertical()
                        .max_height(CHAT_PANEL_MAX_HEIGHT)
                        .stick_to_bottom(true)
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            let mut clicked_location = None;
                            for line in lines.iter().skip(start) {
                                if clicked_location.is_none() {
                                    clicked_location = render_chat_line_with_mentions(
                                        ui,
                                        line.formatted().as_str(),
                                        text_color,
                                        mention_color,
                                        location_color,
                                    );
                                } else {
                                    let _ = render_chat_line_with_mentions(
                                        ui,
                                        line.formatted().as_str(),
                                        text_color,
                                        mention_color,
                                        location_color,
                                    );
                                }
                            }
                            if let Some([x, y, z]) = clicked_location {
                                submit.write(ChatSubmitRequest {
                                    text: format!("/tp {x} {y} {z}"),
                                });
                            }
                        });

                    if chat_ui.open {
                        ui.add_space(6.0);
                        let response = egui::Frame::default()
                            .fill(egui::Color32::TRANSPARENT)
                            .stroke(egui::Stroke::new(2.0, accent_color))
                            .corner_radius(egui::CornerRadius::same(6))
                            .inner_margin(egui::Margin::symmetric(8, 10))
                            .show(ui, |ui| {
                                let input_id = ui.make_persistent_id("oplexa-chat-input");
                                ui.with_layout(
                                    egui::Layout::left_to_right(egui::Align::Center),
                                    |ui| {
                                        ui.add_sized(
                                            [ui.available_width(), CHAT_INPUT_HEIGHT],
                                            egui::TextEdit::singleline(&mut chat_ui.input)
                                                .id(input_id)
                                                .frame(false)
                                                .desired_width(f32::INFINITY)
                                                .font(egui::TextStyle::Monospace)
                                                .hint_text("Type a message or command..."),
                                        )
                                    },
                                )
                                .inner
                            })
                            .inner;

                        if chat_ui.focus_input_next_frame || !response.has_focus() {
                            response.request_focus();
                            chat_ui.focus_input_next_frame = true;
                        }
                        if response.has_focus() {
                            chat_ui.focus_input_next_frame = false;
                        }

                        if let Some(suggestion) = &chat_ui.suggestion {
                            ui.add_space(4.0);
                            ui.label(
                                egui::RichText::new(format!("Suggestion: {}", suggestion))
                                    .color(hint_color)
                                    .monospace()
                                    .size(CHAT_FONT_SIZE),
                            );
                        }
                    }
                });
        });
}

/// Closes chat ui state for the `graphic::components::chat` module.
fn close_chat_ui_on_exit(
    mut chat_ui: ResMut<ChatUiState>,
    mut ui_interaction: ResMut<UiInteractionState>,
    mut chat_log: ResMut<ChatLog>,
) {
    chat_log.clear();
    chat_ui.open = false;
    chat_ui.input.clear();
    chat_ui.suggestion = None;
    chat_ui.focus_input_next_frame = false;
    ui_interaction.chat_open = false;
}

fn render_chat_line_with_mentions(
    ui: &mut egui::Ui,
    text: &str,
    base_color: egui::Color32,
    mention_color: egui::Color32,
    location_color: egui::Color32,
) -> Option<[f32; 3]> {
    let mut clicked_location = None;
    ui.horizontal_wrapped(|ui| {
        let mut cursor = 0usize;
        while cursor < text.len() {
            let next_mention = next_mention_span(text, cursor).map(|(start, end)| {
                ChatSpecialSpan::Mention { start, end }
            });
            let next_location = next_location_span(text, cursor).map(|(start, end, coords)| {
                ChatSpecialSpan::Location { start, end, coords }
            });
            let next = earliest_special_span(next_mention, next_location);

            let Some(next_span) = next else {
                if cursor < text.len() {
                    ui.label(
                        egui::RichText::new(&text[cursor..])
                            .color(base_color)
                            .monospace()
                            .size(CHAT_FONT_SIZE),
                    );
                }
                break;
            };

            let (start, end) = next_span.range();
            if start > cursor {
                ui.label(
                    egui::RichText::new(&text[cursor..start])
                        .color(base_color)
                        .monospace()
                    .size(CHAT_FONT_SIZE),
                );
            }

            match next_span {
                ChatSpecialSpan::Mention { .. } => {
                    ui.label(
                        egui::RichText::new(&text[start..end])
                            .color(mention_color)
                            .strong()
                            .monospace()
                            .size(CHAT_FONT_SIZE),
                    );
                }
                ChatSpecialSpan::Location { coords, .. } => {
                    let response = ui.add(
                        egui::Label::new(
                            egui::RichText::new(&text[start..end])
                                .color(location_color)
                                .strong()
                                .underline()
                                .monospace()
                                .size(CHAT_FONT_SIZE),
                        )
                        .sense(egui::Sense::click()),
                    );
                    if response.clicked() {
                        clicked_location = Some(coords);
                    }
                }
            }
            cursor = end;
        }
    });
    clicked_location
}

#[derive(Clone, Copy, Debug)]
enum ChatSpecialSpan {
    Mention { start: usize, end: usize },
    Location { start: usize, end: usize, coords: [f32; 3] },
}

impl ChatSpecialSpan {
    #[inline]
    fn range(self) -> (usize, usize) {
        match self {
            Self::Mention { start, end } => (start, end),
            Self::Location { start, end, .. } => (start, end),
        }
    }
}

fn earliest_special_span(
    mention: Option<ChatSpecialSpan>,
    location: Option<ChatSpecialSpan>,
) -> Option<ChatSpecialSpan> {
    match (mention, location) {
        (Some(a), Some(b)) => {
            let a_start = a.range().0;
            let b_start = b.range().0;
            if a_start <= b_start { Some(a) } else { Some(b) }
        }
        (Some(span), None) | (None, Some(span)) => Some(span),
        (None, None) => None,
    }
}

fn next_mention_span(text: &str, from: usize) -> Option<(usize, usize)> {
    let mut iter = text[from..].char_indices();
    while let Some((offset, ch)) = iter.next() {
        if ch != '@' {
            continue;
        }
        let start = from + offset;
        if let Some(end) = parse_mention_end(text, start) {
            return Some((start, end));
        }
    }
    None
}

fn parse_mention_end(text: &str, at: usize) -> Option<usize> {
    let rest = text.get(at..)?;
    if !rest.starts_with('@') {
        return None;
    }

    if let Some(body) = rest.strip_prefix("@{") {
        let close_rel = body.find('}')?;
        if close_rel == 0 {
            return None;
        }
        return Some(at + 2 + close_rel + 1);
    }

    let mut end = at + 1;
    for (rel, ch) in rest[1..].char_indices() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            end = at + 1 + rel + ch.len_utf8();
        } else {
            break;
        }
    }
    if end <= at + 1 { None } else { Some(end) }
}

fn next_location_span(text: &str, from: usize) -> Option<(usize, usize, [f32; 3])> {
    let mut cursor = from;
    while cursor < text.len() {
        let rel = text[cursor..].find("&:[")?;
        let start = cursor + rel;
        if let Some((end, coords)) = parse_location_token(text, start) {
            return Some((start, end, coords));
        }
        cursor = start.saturating_add(3);
    }
    None
}

fn parse_location_token(text: &str, start: usize) -> Option<(usize, [f32; 3])> {
    let bytes = text.as_bytes();
    if start + 3 > bytes.len() || &text[start..start + 3] != "&:[" {
        return None;
    }

    let mut idx = start + 3;
    idx = skip_ascii_whitespace(bytes, idx);
    let (x, next_idx) = parse_ascii_f32(text, idx)?;
    idx = skip_ascii_whitespace(bytes, next_idx);
    if bytes.get(idx).copied()? != b',' {
        return None;
    }

    idx = skip_ascii_whitespace(bytes, idx + 1);
    let (y, next_idx) = parse_ascii_f32(text, idx)?;
    idx = skip_ascii_whitespace(bytes, next_idx);
    if bytes.get(idx).copied()? != b',' {
        return None;
    }

    idx = skip_ascii_whitespace(bytes, idx + 1);
    let (z, next_idx) = parse_ascii_f32(text, idx)?;
    idx = skip_ascii_whitespace(bytes, next_idx);
    if bytes.get(idx).copied()? != b']' {
        return None;
    }

    Some((idx + 1, [x, y, z]))
}

#[inline]
fn skip_ascii_whitespace(bytes: &[u8], mut idx: usize) -> usize {
    while let Some(ch) = bytes.get(idx).copied() {
        if !ch.is_ascii_whitespace() {
            break;
        }
        idx += 1;
    }
    idx
}

fn parse_ascii_f32(text: &str, start: usize) -> Option<(f32, usize)> {
    let bytes = text.as_bytes();
    let mut idx = start;
    if matches!(bytes.get(idx).copied(), Some(b'+') | Some(b'-')) {
        idx += 1;
    }

    let mut has_digit = false;
    while matches!(bytes.get(idx).copied(), Some(b'0'..=b'9')) {
        has_digit = true;
        idx += 1;
    }

    if matches!(bytes.get(idx).copied(), Some(b'.')) {
        idx += 1;
        while matches!(bytes.get(idx).copied(), Some(b'0'..=b'9')) {
            has_digit = true;
            idx += 1;
        }
    }

    if !has_digit {
        return None;
    }

    let value = text[start..idx].parse::<f32>().ok()?;
    Some((value, idx))
}
