use smallvec::SmallVec;

use crate::{
    TextComponent,
    content::{Content, NbtSource, Object, ObjectPlayer, Resolvable},
    format::{Color, Format},
    interactivity::{ClickEvent, HoverEvent},
    translation::TranslatedMessage,
};
use std::borrow::Cow;

#[cfg(feature = "custom")]
use crate::custom::{CustomData, Payload};

pub fn parse(input: &str) -> TextComponent {
    Parser::parse(input)
}

fn new_component(content: Content) -> TextComponent {
    TextComponent {
        content,
        ..Default::default()
    }
}

fn join_with_colon(args: &[Cow<str>]) -> String {
    let mut result = String::new();
    for a in args {
        if !result.is_empty() {
            result.push(':');
        }
        result.push_str(a.as_ref());
    }
    result
}

struct Parser<'a> {
    nodes: Vec<TextComponent>,
    children: Vec<Vec<usize>>,
    stack: Vec<(usize, Cow<'a, str>)>,
}

impl<'a> Parser<'a> {
    fn parse(input: &'a str) -> TextComponent {
        let mut parser = Parser {
            nodes: vec![TextComponent::new()],
            children: vec![Vec::new()],
            stack: vec![(0, Cow::Borrowed(""))],
        };
        let len = input.len();
        let bytes = input.as_bytes();
        let mut i = 0;

        while i < len {
            let start = i;
            let next_tag = memchr::memchr(b'<', &bytes[i..]);
            if let Some(offset) = next_tag {
                i += offset;
            } else {
                i = len;
            }
            if i > start {
                let text = unescape_text(&input[start..i]);
                if !text.is_empty() {
                    let comp = TextComponent::plain(text.into_owned());
                    let parent = parser.stack.last().unwrap().0;
                    parser.add_child_node(parent, comp);
                }
            }
            if next_tag.is_none() {
                break;
            }
            i += 1;
            if i >= len {
                break;
            }
            if bytes[i] == b'/' {
                i += 1;
                let end = i;
                while i < len && bytes[i] != b'>' {
                    i += 1;
                }
                let tag_name = &input[end..i];
                if i < len {
                    i += 1;
                }
                parser.close_tag(tag_name);
                continue;
            }
            let tag_start = i;
            while i < len {
                match bytes[i] {
                    b'>' | b':' | b'/' => break,
                    _ => i += 1,
                }
            }
            let tag_name = &input[tag_start..i];
            let mut args = SmallVec::new();
            let mut self_closing = false;

            if i < len && bytes[i] == b':' {
                i += 1;
                args = split_args(input, &mut i, len);
            }
            if i < len && bytes[i] == b'/' {
                self_closing = true;
                i += 1;
            }
            if i < len && bytes[i] == b'>' {
                i += 1;
            } else {
                while i < len && bytes[i] != b'>' {
                    i += 1;
                }
                if i < len {
                    i += 1;
                }
            }

            parser.process_open_tag(tag_name, args, self_closing);
        }

        parser.finish()
    }

    fn add_child_node(&mut self, parent: usize, child: TextComponent) -> usize {
        if let Content::Text { text: child_text } = &child.content
            && child.format == Format::new()
            && child.interactions == Default::default()
            && let Some(&last_idx) = self.children[parent].last()
        {
            let last_node = &mut self.nodes[last_idx];
            if let Content::Text { text: last_text } = &mut last_node.content
                && last_node.format == Format::new()
                && last_node.interactions == Default::default()
            {
                last_text.to_mut().push_str(child_text);
                return last_idx;
            }
        }

        let idx = self.nodes.len();
        self.nodes.push(child);
        self.children.push(Vec::new());
        self.children[parent].push(idx);
        idx
    }

    fn push_tag_to_stack(
        &mut self,
        parent: usize,
        comp: TextComponent,
        tag_name: Option<Cow<'a, str>>,
        self_closing: bool,
    ) -> usize {
        let idx = self.add_child_node(parent, comp);
        if !self_closing && let Some(name) = tag_name {
            self.stack.push((idx, name));
        }
        idx
    }

    fn push_format_tag(
        &mut self,
        parent: usize,
        format: Format,
        self_closing: bool,
        tag: &'a str,
    ) -> usize {
        let comp = TextComponent {
            content: Content::Text {
                text: Cow::Borrowed(""),
            },
            format,
            ..Default::default()
        };
        self.push_tag_to_stack(parent, comp, Some(Cow::Borrowed(tag)), self_closing)
    }

    fn close_tag(&mut self, tag_name: &str) {
        let tag_lower = tag_name.to_lowercase();
        if tag_lower == "reset" {
            return;
        }
        if let Some(pos) = self
            .stack
            .iter()
            .rposition(|(_, name)| name.as_ref().eq_ignore_ascii_case(&tag_lower))
        {
            self.stack.truncate(pos);
        }
    }

    fn process_open_tag(
        &mut self,
        tag_name: &'a str,
        args: SmallVec<[Cow<'a, str>; 4]>,
        self_closing: bool,
    ) {
        let parent = self.stack.last().map(|s| s.0).unwrap_or(0);
        let lower = tag_name.to_lowercase();

        match lower.as_str() {
            "b" | "bold" | "!b" | "!bold" | "i" | "em" | "italic" | "!i" | "!em" | "!italic"
            | "u" | "underlined" | "!u" | "!underlined" | "st" | "strikethrough" | "!st"
            | "!strikethrough" | "obf" | "obfuscated" | "!obf" | "!obfuscated" => {
                let (decoration, value) = if let Some(rest) = lower.strip_prefix('!') {
                    (rest, false)
                } else {
                    (lower.as_str(), true)
                };

                let f = Format::new();
                let format = match decoration {
                    "b" | "bold" => f.bold(value),
                    "i" | "em" | "italic" => f.italic(value),
                    "u" | "underlined" => f.underlined(value),
                    "st" | "strikethrough" => f.strikethrough(value),
                    "obf" | "obfuscated" => f.obfuscated(value),
                    _ => return,
                };

                self.push_format_tag(parent, format, self_closing, tag_name);
            }
            "reset" => self.stack.truncate(1),
            "shadow" => {
                let format = parse_shadow(&args);
                self.push_format_tag(parent, format, self_closing, "shadow");
            }
            "!shadow" => {
                let mut fmt = Format::new();
                fmt.shadow_color = Some(0);
                self.push_format_tag(parent, fmt, self_closing, "!shadow");
            }
            "color" | "c" | "colour" => {
                let color = args.first().and_then(|a| parse_color(a));
                let format = match color {
                    Some(c) => Format::new().color(c),
                    None => Format::new(),
                };
                self.push_format_tag(parent, format, self_closing, tag_name);
            }
            "click" => {
                let mut args = args;
                if let Some(action) = take_first_arg(&mut args) {
                    let value = join_with_colon(&args);
                    let click = parse_click(&action, &value);
                    let mut comp = new_component(Content::Text {
                        text: Cow::Borrowed(""),
                    });
                    comp.interactions.click = click;
                    self.push_tag_to_stack(
                        parent,
                        comp,
                        Some(Cow::Borrowed("click")),
                        self_closing,
                    );
                }
            }
            "hover" => {
                let hover = parse_hover(&args);
                let mut comp = new_component(Content::Text {
                    text: Cow::Borrowed(""),
                });
                comp.interactions.hover = hover;
                self.push_tag_to_stack(parent, comp, Some(Cow::Borrowed("hover")), self_closing);
            }
            "insert" => {
                let mut args = args;
                if let Some(text) = take_first_arg(&mut args) {
                    let mut comp = new_component(Content::Text {
                        text: Cow::Borrowed(""),
                    });
                    comp.interactions.insertion = Some(Cow::Owned(text.into_owned()));
                    self.push_tag_to_stack(
                        parent,
                        comp,
                        Some(Cow::Borrowed("insert")),
                        self_closing,
                    );
                }
            }
            "font" => {
                let font = join_with_colon(&args);
                self.push_format_tag(parent, Format::new().font(font), self_closing, "font");
            }
            "key" => {
                let keybind = join_with_colon(&args);
                let comp = new_component(Content::Keybind {
                    keybind: Cow::Owned(keybind),
                });
                self.add_child_node(parent, comp);
            }
            "lang" | "tr" | "translate" => {
                self.handle_translate_tag(args, parent, None);
            }
            "lang_or" | "tr_or" | "translate_or" => {
                self.handle_translate_tag(args, parent, Some(true));
            }
            "newline" | "br" => {
                self.add_child_node(parent, TextComponent::plain("\n"));
            }
            "selector" | "sel" => {
                self.handle_selector_tag(args, parent);
            }
            "score" => {
                let mut args = args;
                if let (Some(name), Some(objective)) =
                    (take_first_arg(&mut args), take_first_arg(&mut args))
                {
                    let resolvable = Resolvable::Scoreboard {
                        selector: Cow::Owned(name.into_owned()),
                        objective: Cow::Owned(objective.into_owned()),
                    };
                    let comp = new_component(Content::Resolvable(resolvable));
                    self.add_child_node(parent, comp);
                }
            }
            "nbt" | "data" => {
                self.handle_nbt_tag(args, parent);
            }
            "sprite" => {
                if args.is_empty() {
                    return;
                }
                let mut args = args;
                let atlas = if args.len() > 1 {
                    take_first_arg(&mut args)
                } else {
                    None
                };
                let sprite = take_first_arg(&mut args).unwrap();
                let comp = new_component(Content::Object(Object::Atlas {
                    atlas: atlas.map(|a| Cow::Owned(a.into_owned())),
                    sprite: Cow::Owned(sprite.into_owned()),
                }));
                self.add_child_node(parent, comp);
            }
            "head" => {
                self.handle_head_tag(args, parent);
            }
            #[cfg(feature = "custom")]
            "rainbow" => {
                let comp = new_component(Content::Custom(CustomData {
                    id: Cow::Borrowed("rainbow"),
                    payload: Payload::Empty,
                }));
                self.push_tag_to_stack(parent, comp, Some(Cow::Borrowed("rainbow")), self_closing);
            }
            #[cfg(feature = "custom")]
            "gradient" => {
                let comp = new_component(Content::Custom(CustomData {
                    id: Cow::Borrowed("gradient"),
                    payload: Payload::Empty,
                }));
                self.push_tag_to_stack(parent, comp, Some(Cow::Borrowed("gradient")), self_closing);
            }
            #[cfg(feature = "custom")]
            "transition" => {
                let comp = new_component(Content::Custom(CustomData {
                    id: Cow::Borrowed("transition"),
                    payload: Payload::Empty,
                }));
                self.push_tag_to_stack(
                    parent,
                    comp,
                    Some(Cow::Borrowed("transition")),
                    self_closing,
                );
            }
            #[cfg(feature = "custom")]
            "pride" => {
                let comp = new_component(Content::Custom(CustomData {
                    id: Cow::Borrowed("pride"),
                    payload: Payload::Empty,
                }));
                self.push_tag_to_stack(parent, comp, Some(Cow::Borrowed("pride")), self_closing);
            }
            _ => {
                let color = parse_color(&lower).or_else(|| {
                    if lower.starts_with('#') {
                        Color::from_hex(&lower)
                    } else {
                        None
                    }
                });

                if let Some(color) = color {
                    self.push_format_tag(
                        parent,
                        Format::new().color(color),
                        self_closing,
                        tag_name,
                    );
                }
            }
        }
    }

    fn handle_translate_tag(
        &mut self,
        args: SmallVec<[Cow<'a, str>; 4]>,
        parent: usize,
        has_fallback: Option<bool>,
    ) {
        let mut args = args;
        let key = take_first_arg(&mut args);
        let fallback = if has_fallback.is_some() {
            take_first_arg(&mut args).map(|c| Cow::Owned(c.into_owned()))
        } else {
            None
        };

        if let Some(key) = key {
            let t_args: Vec<TextComponent> = args
                .into_iter()
                .map(|a| parse_minimessage(a.as_ref()))
                .collect();
            let msg = TranslatedMessage {
                key: Cow::Owned(key.into_owned()),
                fallback,
                args: if t_args.is_empty() {
                    None
                } else {
                    Some(t_args.into_boxed_slice())
                },
            };
            let comp = new_component(Content::Translate(msg));
            self.add_child_node(parent, comp);
        }
    }

    fn handle_selector_tag(&mut self, args: SmallVec<[Cow<'a, str>; 4]>, parent: usize) {
        let mut args = args;
        if let Some(sel) = take_first_arg(&mut args) {
            let separator = if let Some(sep) = take_first_arg(&mut args) {
                Box::new(parse_minimessage(&sep))
            } else {
                Resolvable::entity_separator()
            };
            let resolvable = Resolvable::Entity {
                selector: Cow::Owned(sel.into_owned()),
                separator,
            };
            let comp = new_component(Content::Resolvable(resolvable));
            self.add_child_node(parent, comp);
        }
    }

    fn handle_nbt_tag(&mut self, args: SmallVec<[Cow<'a, str>; 4]>, parent: usize) {
        let mut args = args;
        if args.len() >= 3 {
            let source_type = take_first_arg(&mut args)
                .map(|c| c.into_owned())
                .unwrap_or_default();
            let id = take_first_arg(&mut args).map(|c| c.into_owned());
            let path = take_first_arg(&mut args).map(|c| c.into_owned());
            if let (Some(id), Some(path)) = (id, path) {
                let sep = take_first_arg(&mut args).map(|c| c.into_owned());
                let separator = if let Some(s) = sep {
                    Box::new(parse_minimessage(&s))
                } else {
                    Resolvable::nbt_separator()
                };
                let interpret = args.first().is_some_and(|v| v.as_ref() == "interpret");
                let source = match source_type.as_str() {
                    "entity" => NbtSource::Entity(Cow::Owned(id)),
                    "block" => NbtSource::Block(Cow::Owned(id)),
                    "storage" => NbtSource::Storage(Cow::Owned(id)),
                    _ => return,
                };
                let resolvable = Resolvable::NBT {
                    path: Cow::Owned(path),
                    interpret: if interpret { Some(true) } else { None },
                    separator,
                    source,
                };
                let comp = new_component(Content::Resolvable(resolvable));
                self.add_child_node(parent, comp);
            }
        }
    }

    fn handle_head_tag(&mut self, args: SmallVec<[Cow<'a, str>; 4]>, parent: usize) {
        let mut args = args;
        if let Some(head_str) = take_first_arg(&mut args) {
            let outer_layer = args.first().is_none_or(|v| v.as_ref() != "false");
            let player = if let Ok(uuid) = uuid::Uuid::parse_str(head_str.as_ref()) {
                let (high, low) = uuid.as_u64_pair();
                let id = [
                    (high >> 32) as i32,
                    high as i32,
                    (low >> 32) as i32,
                    low as i32,
                ];
                ObjectPlayer::id(id)
            } else if head_str.contains('/') || head_str.contains(':') {
                ObjectPlayer::texture(head_str.into_owned())
            } else {
                ObjectPlayer::name(head_str.into_owned())
            };
            let comp = new_component(Content::Object(Object::Player {
                player,
                hat: outer_layer,
            }));
            self.add_child_node(parent, comp);
        }
    }

    fn finish(mut self) -> TextComponent {
        self.stack.truncate(1);
        self.build_node(0)
    }

    fn build_node(&mut self, idx: usize) -> TextComponent {
        let child_indices = std::mem::take(&mut self.children[idx]);
        let mut node = std::mem::take(&mut self.nodes[idx]);
        node.children = child_indices
            .into_iter()
            .map(|cidx| self.build_node(cidx))
            .collect();
        node
    }
}

fn take_first_arg<'a>(args: &mut SmallVec<[Cow<'a, str>; 4]>) -> Option<Cow<'a, str>> {
    if args.is_empty() {
        return None;
    }
    Some(args.remove(0))
}

fn unescape_text(s: &str) -> Cow<'_, str> {
    if !s.contains('\\') {
        return Cow::Borrowed(s);
    }
    let bytes = s.as_bytes();
    let mut result = String::with_capacity(s.len());
    let mut start = 0;
    while let Some(rel_pos) = memchr::memchr(b'\\', &bytes[start..]) {
        let pos = start + rel_pos;
        result.push_str(&s[start..pos]);
        start = pos + 1;
        if start < bytes.len() {
            let next_byte = bytes[start];
            match next_byte {
                b'<' | b'\\' => {
                    result.push(next_byte as char);
                    start += 1;
                }
                other => {
                    result.push('\\');
                    result.push(other as char);
                    start += 1;
                }
            }
        } else {
            result.push('\\');
            break;
        }
    }
    if start < s.len() {
        result.push_str(&s[start..]);
    }
    Cow::Owned(result)
}

fn split_args<'a>(input: &'a str, pos: &mut usize, max: usize) -> SmallVec<[Cow<'a, str>; 4]> {
    let mut args = SmallVec::new();
    let bytes = input.as_bytes();
    while *pos < max {
        let start = *pos;
        if bytes[*pos] == b'"' || bytes[*pos] == b'\'' {
            let quote = bytes[*pos];
            *pos += 1;
            let content_start = *pos;
            let mut escaped = String::new();
            let mut has_escape = false;
            loop {
                let rem = &bytes[*pos..max];
                match memchr::memchr2(quote, b'\\', rem) {
                    None => {
                        if has_escape {
                            escaped.push_str(&input[content_start..max]);
                            args.push(Cow::Owned(escaped));
                        } else {
                            args.push(Cow::Borrowed(&input[content_start..max]));
                        }
                        *pos = max;
                        break;
                    }
                    Some(offset) => {
                        let found = rem[offset];
                        let abs_pos = *pos + offset;
                        if found == quote {
                            if has_escape {
                                escaped.push_str(&input[*pos..abs_pos]);
                                args.push(Cow::Owned(escaped));
                            } else {
                                args.push(Cow::Borrowed(&input[*pos..abs_pos]));
                            }
                            *pos = abs_pos + 1;
                            break;
                        } else {
                            has_escape = true;
                            if escaped.is_empty() {
                                escaped.push_str(&input[content_start..abs_pos]);
                            } else {
                                escaped.push_str(&input[*pos..abs_pos]);
                            }
                            *pos = abs_pos + 1;
                            if *pos < max {
                                let next_byte = bytes[*pos];
                                match next_byte {
                                    b'\\' => escaped.push('\\'),
                                    b'"' if quote == b'"' => escaped.push('"'),
                                    b'\'' if quote == b'\'' => escaped.push('\''),
                                    c => {
                                        escaped.push('\\');
                                        escaped.push(c as char);
                                    }
                                }
                                *pos += 1;
                            } else {
                                escaped.push('\\');
                                args.push(Cow::Owned(escaped));
                                *pos = max;
                                break;
                            }
                        }
                    }
                }
            }
            if *pos < max && bytes[*pos] == b':' {
                *pos += 1;
                continue;
            } else {
                break;
            }
        } else {
            if let Some(offset) = memchr::memchr2(b':', b'>', &bytes[*pos..max]) {
                let idx = *pos + offset;
                let found = bytes[idx];
                if found == b'>' {
                    if start < idx {
                        args.push(Cow::Borrowed(&input[start..idx]));
                    }
                    *pos = idx;
                    break;
                } else {
                    args.push(Cow::Borrowed(&input[start..idx]));
                    *pos = idx + 1;
                    continue;
                }
            } else {
                args.push(Cow::Borrowed(&input[start..max]));
                *pos = max;
                break;
            }
        }
    }
    args
}

fn parse_color(s: &str) -> Option<Color> {
    Color::from_hex(s).or_else(|| {
        let color = match s {
            "black" => Color::Black,
            "dark_blue" => Color::DarkBlue,
            "dark_green" => Color::DarkGreen,
            "dark_aqua" => Color::DarkAqua,
            "dark_red" => Color::DarkRed,
            "dark_purple" => Color::DarkPurple,
            "gold" => Color::Gold,
            "gray" | "grey" => Color::Gray,
            "dark_gray" | "dark_grey" => Color::DarkGray,
            "blue" => Color::Blue,
            "green" => Color::Green,
            "aqua" => Color::Aqua,
            "red" => Color::Red,
            "light_purple" => Color::LightPurple,
            "yellow" => Color::Yellow,
            "white" => Color::White,
            _ => return None,
        };
        Some(color)
    })
}

fn color_to_rgb(color: &Color) -> (u8, u8, u8) {
    match color {
        Color::Black => (0, 0, 0),
        Color::DarkBlue => (0, 0, 170),
        Color::DarkGreen => (0, 170, 0),
        Color::DarkAqua => (0, 170, 170),
        Color::DarkRed => (170, 0, 0),
        Color::DarkPurple => (170, 0, 170),
        Color::Gold => (255, 170, 0),
        Color::Gray => (170, 170, 170),
        Color::DarkGray => (85, 85, 85),
        Color::Blue => (85, 85, 255),
        Color::Green => (85, 255, 85),
        Color::Aqua => (85, 255, 255),
        Color::Red => (255, 85, 85),
        Color::LightPurple => (255, 85, 255),
        Color::Yellow => (255, 255, 85),
        Color::White => (255, 255, 255),
        Color::Rgb(r, g, b) => (*r, *g, *b),
    }
}

fn parse_click(action: &str, value: &str) -> Option<ClickEvent> {
    let value_str = value.to_string();
    let event = match action {
        "open_url" => ClickEvent::OpenUrl {
            url: Cow::Owned(value_str.clone()),
        },
        "run_command" => ClickEvent::RunCommand {
            command: Cow::Owned(value_str.clone()),
        },
        "suggest_command" => ClickEvent::SuggestCommand {
            command: Cow::Owned(value_str.clone()),
        },
        "change_page" => {
            let page = value.parse::<i32>().ok()?;
            ClickEvent::ChangePage { page }
        }
        "copy_to_clipboard" => ClickEvent::CopyToClipboard {
            value: Cow::Owned(value_str.clone()),
        },
        "show_dialog" => ClickEvent::ShowDialog {
            dialog: Cow::Owned(value_str.clone()),
        },
        #[cfg(feature = "custom")]
        "custom" => ClickEvent::Custom(CustomData {
            id: Cow::Owned(value_str),
            payload: Payload::Empty,
        }),
        _ => return None,
    };
    Some(event)
}

fn parse_hover(args: &[Cow<str>]) -> Option<HoverEvent> {
    let first = args.first()?.as_ref();
    match first {
        "show_text" => {
            let text = parse_minimessage(args.get(1)?.as_ref());
            Some(HoverEvent::ShowText {
                value: Box::new(text),
            })
        }
        "show_item" => {
            let id = args.get(1)?.to_string();
            let count = args.get(2).and_then(|s| s.parse::<i32>().ok());
            let components = args.get(3).map(|s| Cow::Owned(s.to_string()));
            Some(HoverEvent::ShowItem {
                id: Cow::Owned(id),
                count,
                components,
            })
        }
        "show_entity" => {
            let id = args.get(1)?.to_string();
            let uuid = uuid::Uuid::parse_str(args.get(2)?.as_ref()).ok()?;
            let name = args.get(3).map(|s| Box::new(parse_minimessage(s.as_ref())));
            Some(HoverEvent::ShowEntity {
                name,
                id: Cow::Owned(id),
                uuid,
            })
        }
        _ => None,
    }
}

fn parse_minimessage(s: &str) -> TextComponent {
    parse(s)
}

fn parse_shadow(args: &[Cow<str>]) -> Format {
    let mut format = Format::new();
    if args.is_empty() {
        return format;
    }
    let color_arg = &args[0];
    let alpha_from_args = |idx: usize| {
        args.get(idx)
            .and_then(|a| a.parse::<f32>().ok())
            .map(|f| (f * 255.0).round() as u8)
    };
    let shadow = if let Some(hex) = color_arg.strip_prefix('#') {
        if hex.len() == 8 {
            if let (Ok(r), Ok(g), Ok(b), Ok(a)) = (
                u8::from_str_radix(&hex[0..2], 16),
                u8::from_str_radix(&hex[2..4], 16),
                u8::from_str_radix(&hex[4..6], 16),
                u8::from_str_radix(&hex[6..8], 16),
            ) {
                Some(Format::parse_shadow_color(a, r, g, b))
            } else {
                None
            }
        } else if hex.len() == 6 {
            if let (Ok(r), Ok(g), Ok(b)) = (
                u8::from_str_radix(&hex[0..2], 16),
                u8::from_str_radix(&hex[2..4], 16),
                u8::from_str_radix(&hex[4..6], 16),
            ) {
                let a = alpha_from_args(1).unwrap_or(64);
                Some(Format::parse_shadow_color(a, r, g, b))
            } else {
                None
            }
        } else {
            None
        }
    } else if let Some(color) = parse_color(color_arg) {
        let (r, g, b) = color_to_rgb(&color);
        let a = alpha_from_args(1).unwrap_or(64);
        Some(Format::parse_shadow_color(a, r, g, b))
    } else {
        None
    };
    if let Some(s) = shadow {
        format.shadow_color = Some(s);
    }
    format
}
