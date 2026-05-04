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

struct Parser {
    nodes: Vec<TextComponent>,
    children: Vec<Vec<usize>>,
    stack: Vec<(usize, String)>,
}

impl Parser {
    fn parse(input: &str) -> TextComponent {
        let mut parser = Parser {
            nodes: vec![TextComponent::new()],
            children: vec![Vec::new()],
            stack: vec![(0, String::new())],
        };
        let len = input.len();
        let bytes = input.as_bytes();
        let mut i = 0;

        while i < len {
            let start = i;
            if let Some(offset) = memchr::memchr(b'<', &bytes[i..]) {
                i += offset;
                if i > start {
                    let text = unescape_text(&input[start..i]);
                    if !text.is_empty() {
                        let comp = TextComponent::plain(text.into_owned());
                        let parent = parser.stack.last().unwrap().0;
                        parser.add_child_node(parent, comp);
                    }
                }
                i += 1;
            } else {
                if i < len {
                    let text = unescape_text(&input[i..]);
                    if !text.is_empty() {
                        let comp = TextComponent::plain(text.into_owned());
                        let parent = parser.stack.last().unwrap().0;
                        parser.add_child_node(parent, comp);
                    }
                }
                break;
            }

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
        tag_name: Option<String>,
        self_closing: bool,
    ) -> usize {
        let idx = self.add_child_node(parent, comp);
        if !self_closing && let Some(name) = tag_name {
            self.stack.push((idx, name));
        }
        idx
    }

    fn push_format_wrapper(&mut self, parent: usize, format: Format, self_closing: bool) -> usize {
        let comp = TextComponent {
            content: Content::Text {
                text: Cow::Borrowed(""),
            },
            format,
            ..Default::default()
        };
        self.push_tag_to_stack(parent, comp, None, self_closing)
    }

    fn close_tag(&mut self, tag_name: &str) {
        let tag = tag_name.to_lowercase();
        if tag == "reset" {
            return;
        }
        if let Some(pos) = self.stack.iter().rposition(|(_, name)| *name == tag) {
            self.stack.truncate(pos);
        }
    }

    fn process_open_tag(
        &mut self,
        tag_name: &str,
        args: SmallVec<[Cow<'_, str>; 4]>,
        self_closing: bool,
    ) {
        let parent = self.stack.last().map(|s| s.0).unwrap_or(0);
        let tag_lower = tag_name.to_lowercase();

        match tag_lower.as_str() {
            "b" | "bold" | "!b" | "!bold" | "i" | "em" | "italic" | "!i" | "!em" | "!italic"
            | "u" | "underlined" | "!u" | "!underlined" | "st" | "strikethrough" | "!st"
            | "!strikethrough" | "obf" | "obfuscated" | "!obf" | "!obfuscated" => {
                self.handle_decoration_tag(&tag_lower, parent, self_closing)
            }
            "reset" => self.stack.truncate(1),
            "shadow" => self.handle_shadow_tag(args, parent, self_closing),
            "!shadow" => {
                let mut fmt = Format::new();
                fmt.shadow_color = Some(0);
                let idx = self.push_format_wrapper(parent, fmt, self_closing);
                if !self_closing {
                    self.stack.push((idx, "!shadow".to_string()));
                }
            }
            "color" | "c" | "colour" => {
                self.handle_verbose_color_tag(tag_lower, args, parent, self_closing)
            }
            "click" => self.handle_click_tag(args, parent, self_closing),
            "hover" => self.handle_hover_tag(args, parent, self_closing),
            "insert" => self.handle_insertion_tag(args, parent, self_closing),
            "font" => self.handle_font_tag(args, parent, self_closing),
            "key" => self.handle_keybind_tag(args, parent),
            "lang" | "tr" | "translate" => self.handle_translate_tag(args, parent, None),
            "lang_or" | "tr_or" | "translate_or" => {
                self.handle_translate_tag(args, parent, Some(true))
            }
            "newline" | "br" => {
                self.add_child_node(parent, TextComponent::plain("\n"));
            }
            "selector" | "sel" => self.handle_selector_tag(args, parent),
            "score" => self.handle_score_tag(args, parent),
            "nbt" | "data" => self.handle_nbt_tag(args, parent),
            "sprite" => self.handle_sprite_tag(args, parent),
            "head" => self.handle_head_tag(args, parent),
            #[cfg(feature = "custom")]
            "rainbow" => self.push_custom_tag(parent, "rainbow", tag_lower, self_closing),
            #[cfg(feature = "custom")]
            "gradient" => self.push_custom_tag(parent, "gradient", tag_lower, self_closing),
            #[cfg(feature = "custom")]
            "transition" => self.push_custom_tag(parent, "transition", tag_lower, self_closing),
            #[cfg(feature = "custom")]
            "pride" => self.push_custom_tag(parent, "pride", tag_lower, self_closing),
            _ => {
                let color = parse_color(&tag_lower).or_else(|| {
                    if tag_lower.starts_with('#') {
                        Color::from_hex(&tag_lower)
                    } else {
                        None
                    }
                });

                if let Some(color) = color {
                    let idx =
                        self.push_format_wrapper(parent, Format::new().color(color), self_closing);
                    if !self_closing {
                        self.stack.push((idx, tag_lower.to_string()));
                    }
                }
            }
        }
    }

    #[cfg(feature = "custom")]
    fn push_custom_tag(&mut self, parent: usize, id: &str, tag: String, self_closing: bool) {
        let comp = new_component(Content::Custom(CustomData {
            id: Cow::Borrowed(id),
            payload: Payload::Empty,
        }));
        self.push_tag_to_stack(
            parent,
            comp,
            if self_closing { None } else { Some(tag) },
            self_closing,
        );
    }

    fn handle_decoration_tag(&mut self, tag: &str, parent: usize, self_closing: bool) {
        let (decoration, value) = if let Some(rest) = tag.strip_prefix('!') {
            (rest, false)
        } else {
            (tag, true)
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

        let idx = self.push_format_wrapper(parent, format, self_closing);
        if !self_closing {
            self.stack.push((idx, tag.to_string()));
        }
    }

    fn handle_shadow_tag(
        &mut self,
        args: SmallVec<[Cow<'_, str>; 4]>,
        parent: usize,
        self_closing: bool,
    ) {
        let format = parse_shadow(&args);
        let idx = self.push_format_wrapper(parent, format, self_closing);
        if !self_closing {
            self.stack.push((idx, "shadow".to_string()));
        }
    }

    fn handle_verbose_color_tag(
        &mut self,
        tag: String,
        args: SmallVec<[Cow<'_, str>; 4]>,
        parent: usize,
        self_closing: bool,
    ) {
        let color = args.first().and_then(|a| parse_color(a));
        let format = match color {
            Some(c) => Format::new().color(c),
            None => Format::new(),
        };
        let idx = self.push_format_wrapper(parent, format, self_closing);
        if !self_closing {
            self.stack.push((idx, tag));
        }
    }

    fn handle_click_tag(
        &mut self,
        args: SmallVec<[Cow<'_, str>; 4]>,
        parent: usize,
        self_closing: bool,
    ) {
        if args.len() >= 2 {
            let action = take_arg(args[0].clone());
            let value: String = args[1..].iter().fold(String::new(), |mut acc, a| {
                if !acc.is_empty() {
                    acc.push(':');
                }
                acc.push_str(a.as_ref());
                acc
            });
            let click = parse_click(&action, &value);
            let comp = new_component(Content::Text {
                text: Cow::Borrowed(""),
            });
            let mut comp = comp;
            comp.interactions.click = click;
            let tag_name = if self_closing {
                None
            } else {
                Some("click".to_string())
            };
            self.push_tag_to_stack(parent, comp, tag_name, self_closing);
        }
    }

    fn handle_hover_tag(
        &mut self,
        args: SmallVec<[Cow<'_, str>; 4]>,
        parent: usize,
        self_closing: bool,
    ) {
        let hover = parse_hover(&args);
        let mut comp = new_component(Content::Text {
            text: Cow::Borrowed(""),
        });
        comp.interactions.hover = hover;
        let tag_name = if self_closing {
            None
        } else {
            Some("hover".to_string())
        };
        self.push_tag_to_stack(parent, comp, tag_name, self_closing);
    }

    fn handle_insertion_tag(
        &mut self,
        args: SmallVec<[Cow<'_, str>; 4]>,
        parent: usize,
        self_closing: bool,
    ) {
        if let Some(text) = args.first() {
            let mut comp = new_component(Content::Text {
                text: Cow::Borrowed(""),
            });
            comp.interactions.insertion = Some(Cow::Owned(take_arg(text.clone())));
            let tag_name = if self_closing {
                None
            } else {
                Some("insert".to_string())
            };
            self.push_tag_to_stack(parent, comp, tag_name, self_closing);
        }
    }

    fn handle_font_tag(
        &mut self,
        args: SmallVec<[Cow<'_, str>; 4]>,
        parent: usize,
        self_closing: bool,
    ) {
        let font = args.into_iter().fold(String::new(), |mut acc, a| {
            if !acc.is_empty() {
                acc.push(':');
            }
            acc.push_str(a.as_ref());
            acc
        });
        let idx = self.push_format_wrapper(parent, Format::new().font(font), self_closing);
        if !self_closing {
            self.stack.push((idx, "font".to_string()));
        }
    }

    fn handle_keybind_tag(&mut self, args: SmallVec<[Cow<'_, str>; 4]>, parent: usize) {
        let keybind = args.into_iter().fold(String::new(), |mut acc, a| {
            if !acc.is_empty() {
                acc.push(':');
            }
            acc.push_str(a.as_ref());
            acc
        });
        let comp = new_component(Content::Keybind {
            keybind: Cow::Owned(keybind),
        });
        self.add_child_node(parent, comp);
    }

    fn handle_translate_tag(
        &mut self,
        args: SmallVec<[Cow<'_, str>; 4]>,
        parent: usize,
        has_fallback: Option<bool>,
    ) {
        let mut args = args;
        let (key, fallback) = match has_fallback {
            None => (take_first_arg(&mut args), None),
            Some(_) => {
                let key = take_first_arg(&mut args);
                let fb = take_first_arg(&mut args).map(Cow::Owned);
                (key, fb)
            }
        };

        if let Some(key) = key {
            let t_args: Vec<TextComponent> = args
                .into_iter()
                .map(|a| parse_minimessage(a.as_ref()))
                .collect();
            let msg = TranslatedMessage {
                key: Cow::Owned(key),
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

    fn handle_selector_tag(&mut self, args: SmallVec<[Cow<'_, str>; 4]>, parent: usize) {
        let mut args = args;
        if let Some(sel) = take_first_arg(&mut args) {
            let separator = if let Some(sep) = take_first_arg(&mut args) {
                Box::new(parse_minimessage(&sep))
            } else {
                Resolvable::entity_separator()
            };
            let resolvable = Resolvable::Entity {
                selector: Cow::Owned(sel),
                separator,
            };
            let comp = new_component(Content::Resolvable(resolvable));
            self.add_child_node(parent, comp);
        }
    }

    fn handle_score_tag(&mut self, args: SmallVec<[Cow<'_, str>; 4]>, parent: usize) {
        let mut args = args;
        if let (Some(name), Some(objective)) =
            (take_first_arg(&mut args), take_first_arg(&mut args))
        {
            let resolvable = Resolvable::Scoreboard {
                selector: Cow::Owned(name),
                objective: Cow::Owned(objective),
            };
            let comp = new_component(Content::Resolvable(resolvable));
            self.add_child_node(parent, comp);
        }
    }

    fn handle_nbt_tag(&mut self, args: SmallVec<[Cow<'_, str>; 4]>, parent: usize) {
        let args = args;
        if args.len() >= 3 {
            let source_type = take_arg(args[0].clone());
            let id = take_arg(args[1].clone());
            let path = take_arg(args[2].clone());
            let separator = if args.get(3).is_some() {
                let sep = take_arg(args[3].clone());
                Box::new(parse_minimessage(&sep))
            } else {
                Resolvable::nbt_separator()
            };
            let interpret = args.get(4).is_some_and(|v| v.as_ref() == "interpret");
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

    fn handle_sprite_tag(&mut self, args: SmallVec<[Cow<'_, str>; 4]>, parent: usize) {
        let mut args = args;
        let (atlas, sprite) = if args.len() == 1 {
            (None, take_first_arg(&mut args).unwrap_or_default())
        } else if args.len() >= 2 {
            let atlas = take_first_arg(&mut args);
            let sprite = take_first_arg(&mut args).unwrap_or_default();
            (atlas, sprite)
        } else {
            return;
        };
        let comp = new_component(Content::Object(Object::Atlas {
            atlas: atlas.map(Cow::Owned),
            sprite: Cow::Owned(sprite),
        }));
        self.add_child_node(parent, comp);
    }

    fn handle_head_tag(&mut self, args: SmallVec<[Cow<'_, str>; 4]>, parent: usize) {
        let mut args = args;
        if let Some(head_str) = take_first_arg(&mut args) {
            let outer_layer = args.first().is_none_or(|v| v.as_ref() != "false");
            let player = if let Ok(uuid) = uuid::Uuid::parse_str(&head_str) {
                let (high, low) = uuid.as_u64_pair();
                let id = [
                    (high >> 32) as i32,
                    high as i32,
                    (low >> 32) as i32,
                    low as i32,
                ];
                ObjectPlayer::id(id)
            } else if head_str.contains('/') || head_str.contains(':') {
                ObjectPlayer::texture(head_str)
            } else {
                ObjectPlayer::name(head_str)
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

fn take_arg(cow: Cow<'_, str>) -> String {
    match cow {
        Cow::Borrowed(s) => (*s).to_string(),
        Cow::Owned(s) => s.clone(),
    }
}

fn take_first_arg(args: &mut SmallVec<[Cow<'_, str>; 4]>) -> Option<String> {
    if args.is_empty() {
        return None;
    }
    let cow = args.remove(0);
    Some(match cow {
        Cow::Borrowed(s) => s.to_string(),
        Cow::Owned(s) => s,
    })
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
    match action {
        "open_url" => Some(ClickEvent::OpenUrl {
            url: Cow::Owned(value.to_string()),
        }),
        "run_command" => Some(ClickEvent::RunCommand {
            command: Cow::Owned(value.to_string()),
        }),
        "suggest_command" => Some(ClickEvent::SuggestCommand {
            command: Cow::Owned(value.to_string()),
        }),
        "change_page" => value
            .parse::<i32>()
            .ok()
            .map(|page| ClickEvent::ChangePage { page }),
        "copy_to_clipboard" => Some(ClickEvent::CopyToClipboard {
            value: Cow::Owned(value.to_string()),
        }),
        "show_dialog" => Some(ClickEvent::ShowDialog {
            dialog: Cow::Owned(value.to_string()),
        }),
        #[cfg(feature = "custom")]
        "custom" => Some(ClickEvent::Custom(CustomData {
            id: Cow::Owned(value.to_string()),
            payload: Payload::Empty,
        })),
        _ => None,
    }
}

fn parse_hover(args: &[Cow<str>]) -> Option<HoverEvent> {
    match args.first()?.as_ref() {
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
    if let Some(hex) = color_arg.strip_prefix('#') {
        if hex.len() == 8 {
            if let (Ok(r), Ok(g), Ok(b), Ok(a)) = (
                u8::from_str_radix(&hex[0..2], 16),
                u8::from_str_radix(&hex[2..4], 16),
                u8::from_str_radix(&hex[4..6], 16),
                u8::from_str_radix(&hex[6..8], 16),
            ) {
                format.shadow_color = Some(Format::parse_shadow_color(a, r, g, b));
                return format;
            }
        } else if hex.len() == 6 {
            let alpha = args
                .get(1)
                .and_then(|a| a.parse::<f32>().ok())
                .map(|f| (f * 255.0).round() as u8)
                .unwrap_or(64);
            if let (Ok(r), Ok(g), Ok(b)) = (
                u8::from_str_radix(&hex[0..2], 16),
                u8::from_str_radix(&hex[2..4], 16),
                u8::from_str_radix(&hex[4..6], 16),
            ) {
                format.shadow_color = Some(Format::parse_shadow_color(alpha, r, g, b));
                return format;
            }
        }
    } else if let Some(color) = parse_color(color_arg) {
        let (r, g, b) = color_to_rgb(&color);
        let alpha = args
            .get(1)
            .and_then(|a| a.parse::<f32>().ok())
            .map(|f| (f * 255.0).round() as u8)
            .unwrap_or(64);
        format.shadow_color = Some(Format::parse_shadow_color(alpha, r, g, b));
    }
    format
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::{Content, NbtSource, Object, Resolvable};
    use crate::format::{Color, Format};
    use crate::interactivity::{ClickEvent, HoverEvent};

    fn first_child(comp: &TextComponent) -> &TextComponent {
        comp.children.first().expect("expected at least one child")
    }

    fn children(comp: &TextComponent) -> &[TextComponent] {
        &comp.children
    }

    #[test]
    fn plain_text() {
        let root = parse("Hello");
        let child = first_child(&root);
        assert_eq!(
            child.content,
            Content::Text {
                text: Cow::Borrowed("Hello")
            }
        );
        assert!(child.format.color.is_none());
        assert!(child.format.bold.is_none());
        assert!(child.interactions.click.is_none());
    }

    #[test]
    fn color_named() {
        let root = parse("<red>Test");
        let child = first_child(&root);
        assert_eq!(child.format.color, Some(Color::Red));
        assert_eq!(child.children.len(), 1);
        assert_eq!(
            child.children[0].content,
            Content::Text {
                text: Cow::Borrowed("Test")
            }
        );
    }

    #[test]
    fn color_hex() {
        let root = parse("<#00ff00>Green");
        let child = first_child(&root);
        assert_eq!(child.format.color, Some(Color::Rgb(0, 255, 0)));
    }

    #[test]
    fn color_nested() {
        let root = parse("<yellow>Hello <blue>World</blue>!");
        let top_child = first_child(&root);
        assert_eq!(
            top_child.content,
            Content::Text {
                text: Cow::Borrowed("")
            }
        );
        assert_eq!(top_child.format.color, Some(Color::Yellow));
        assert_eq!(top_child.children.len(), 3);

        let hello = &top_child.children[0];
        assert_eq!(
            hello.content,
            Content::Text {
                text: Cow::Borrowed("Hello ")
            }
        );

        let blue_wrapper = &top_child.children[1];
        assert_eq!(blue_wrapper.format.color, Some(Color::Blue));
        assert_eq!(blue_wrapper.children.len(), 1);
        let world = &blue_wrapper.children[0];
        assert_eq!(
            world.content,
            Content::Text {
                text: Cow::Borrowed("World")
            }
        );

        let excl = &top_child.children[2];
        assert_eq!(
            excl.content,
            Content::Text {
                text: Cow::Borrowed("!")
            }
        );
    }

    #[test]
    fn bold() {
        let root = parse("<bold>Bold text");
        let child = first_child(&root);
        assert_eq!(child.format.bold, Some(true));
    }

    #[test]
    fn not_bold() {
        let root = parse("<!bold>Not bold");
        let child = first_child(&root);
        assert_eq!(child.format.bold, Some(false));
    }

    #[test]
    fn italic_aliases() {
        for tag in &["i", "em", "italic"] {
            let root = parse(&format!("<{}>Italic</{}>", tag, tag));
            let child = first_child(&root);
            assert_eq!(child.format.italic, Some(true), "failed for tag {}", tag);
        }
    }

    #[test]
    fn underlined() {
        let root = parse("<u>Under</u>");
        let child = first_child(&root);
        assert_eq!(child.format.underlined, Some(true));
    }

    #[test]
    fn strikethrough() {
        let root = parse("<st>Strike</st>");
        let child = first_child(&root);
        assert_eq!(child.format.strikethrough, Some(true));
    }

    #[test]
    fn obfuscated() {
        let root = parse("<obf>Obfuscated</obf>");
        let child = first_child(&root);
        assert_eq!(child.format.obfuscated, Some(true));
    }

    #[test]
    fn negation_underlined() {
        let root = parse("<!u>Not underlined");
        let child = first_child(&root);
        assert_eq!(child.format.underlined, Some(false));
    }

    #[test]
    fn reset_clears_style() {
        let root = parse("<yellow><bold>Hello <reset>world!");
        let kids = children(&root);
        assert_eq!(kids.len(), 2);

        let yellow = &kids[0];
        assert_eq!(yellow.format.color, Some(Color::Yellow));
        assert!(yellow.format.bold.is_none());
        assert_eq!(yellow.children.len(), 1);

        let bold = &yellow.children[0];
        assert_eq!(bold.format.bold, Some(true));
        assert_eq!(bold.children.len(), 1);
        assert_eq!(
            bold.children[0].content,
            Content::Text {
                text: Cow::Borrowed("Hello ")
            }
        );

        let world = &kids[1];
        assert!(world.format.color.is_none());
        assert!(world.format.bold.is_none());
        assert_eq!(
            world.content,
            Content::Text {
                text: Cow::Borrowed("world!")
            }
        );
    }

    #[test]
    fn shadow_named() {
        let root = parse("<shadow:red>Shadow");
        let child = first_child(&root);
        let expected = Format::parse_shadow_color(64, 255, 85, 85);
        assert_eq!(child.format.shadow_color, Some(expected));
    }

    #[test]
    fn shadow_alpha() {
        let root = parse("<shadow:aqua:0.5>Test");
        let child = first_child(&root);
        let expected = Format::parse_shadow_color(128, 85, 255, 255);
        assert_eq!(child.format.shadow_color, Some(expected));
    }

    #[test]
    fn shadow_hex() {
        let root = parse("<shadow:#FF0000>Red shadow");
        let child = first_child(&root);
        let expected = Format::parse_shadow_color(64, 255, 0, 0);
        assert_eq!(child.format.shadow_color, Some(expected));
    }

    #[test]
    fn shadow_hex_with_alpha() {
        let root = parse("<shadow:#FF000080>Red shadow alpha");
        let child = first_child(&root);
        let expected = Format::parse_shadow_color(0x80, 255, 0, 0);
        assert_eq!(child.format.shadow_color, Some(expected));
    }

    #[test]
    fn shadow_disable() {
        let root = parse("<!shadow>No shadow");
        let child = first_child(&root);
        assert_eq!(child.format.shadow_color, Some(0));
    }

    #[test]
    fn verbose_color() {
        for tag in &["color", "c", "colour"] {
            let root = parse(&format!("<{}:blue>Blue</{}>", tag, tag));
            let child = first_child(&root);
            assert_eq!(child.format.color, Some(Color::Blue), "tag {}", tag);
        }
    }

    #[test]
    fn click_run_command() {
        let root = parse("<click:run_command:/seed>Click");
        let child = first_child(&root);
        assert_eq!(
            child.interactions.click,
            Some(ClickEvent::RunCommand {
                command: Cow::Owned("/seed".into())
            })
        );
    }

    #[test]
    fn click_open_url() {
        let root = parse("<click:open_url:https://example.com>Link");
        let child = first_child(&root);
        assert_eq!(
            child.interactions.click,
            Some(ClickEvent::OpenUrl {
                url: Cow::Owned("https://example.com".into())
            })
        );
    }

    #[test]
    fn click_suggest_command() {
        let root = parse("<click:suggest_command:/help>Suggest");
        let child = first_child(&root);
        assert_eq!(
            child.interactions.click,
            Some(ClickEvent::SuggestCommand {
                command: Cow::Owned("/help".into())
            })
        );
    }

    #[test]
    fn click_change_page() {
        let root = parse("<click:change_page:3>Page 3");
        let child = first_child(&root);
        assert_eq!(
            child.interactions.click,
            Some(ClickEvent::ChangePage { page: 3 })
        );
    }

    #[test]
    fn click_copy_to_clipboard() {
        let root = parse("<click:copy_to_clipboard:secret>Copy");
        let child = first_child(&root);
        assert_eq!(
            child.interactions.click,
            Some(ClickEvent::CopyToClipboard {
                value: Cow::Owned("secret".into())
            })
        );
    }

    #[test]
    fn click_show_dialog() {
        let root = parse("<click:show_dialog:dialog_id>Dialog");
        let child = first_child(&root);
        assert_eq!(
            child.interactions.click,
            Some(ClickEvent::ShowDialog {
                dialog: Cow::Owned("dialog_id".into())
            })
        );
    }

    #[cfg(feature = "custom")]
    #[test]
    fn click_custom() {
        let root = parse("<click:custom:my_action>Custom");
        let child = first_child(&root);
        match &child.interactions.click {
            Some(ClickEvent::Custom(data)) => {
                assert_eq!(data.id, "my_action");
            }
            _ => panic!("expected custom click event"),
        }
    }

    #[test]
    fn hover_show_text() {
        let root = parse("<hover:show_text:'<red>test'>Hover");
        let child = first_child(&root);
        match &child.interactions.hover {
            Some(HoverEvent::ShowText { value }) => {
                let inner = value;
                let inner_child = inner.children.first().unwrap();
                assert_eq!(inner_child.format.color, Some(Color::Red));
                assert_eq!(
                    inner_child.children[0].content,
                    Content::Text {
                        text: Cow::Borrowed("test")
                    }
                );
            }
            _ => panic!("expected show_text hover event"),
        }
    }

    #[test]
    fn hover_show_item() {
        let root = parse("<hover:show_item:stone:3:tag>Item");
        let child = first_child(&root);
        assert_eq!(
            child.interactions.hover,
            Some(HoverEvent::ShowItem {
                id: Cow::Owned("stone".into()),
                count: Some(3),
                components: Some(Cow::Owned("tag".into())),
            })
        );
    }

    #[test]
    fn hover_show_entity() {
        let uuid_str = "1f085b2d-9548-4159-a8c7-f3ccdf0c2054";
        let root = parse(&format!("<hover:show_entity:cow:{}:Name>Entity", uuid_str));
        let child = first_child(&root);
        match &child.interactions.hover {
            Some(HoverEvent::ShowEntity { id, uuid, name }) => {
                assert_eq!(id.as_ref(), "cow");
                assert_eq!(*uuid, uuid::Uuid::parse_str(uuid_str).unwrap());
                let name_comp = name.as_ref().unwrap();
                let name_text = name_comp.children.first().unwrap();
                assert_eq!(
                    name_text.content,
                    Content::Text {
                        text: Cow::Borrowed("Name")
                    }
                );
            }
            _ => panic!("expected show_entity hover event"),
        }
    }

    #[test]
    fn insertion() {
        let root = parse("<insert:test>Insert");
        let child = first_child(&root);
        assert_eq!(
            child.interactions.insertion,
            Some(Cow::Owned("test".into()))
        );
    }

    #[test]
    fn font() {
        let root = parse("<font:uniform>Uniform text");
        let child = first_child(&root);
        assert_eq!(child.format.font, Some(Cow::Owned("uniform".into())));
    }

    #[test]
    fn font_with_namespace() {
        let root = parse("<font:myfont:custom_font>Custom");
        let child = first_child(&root);
        assert_eq!(
            child.format.font,
            Some(Cow::Owned("myfont:custom_font".into()))
        );
    }

    #[test]
    fn keybind() {
        let root = parse("<key:key.jump>");
        let child = first_child(&root);
        assert_eq!(
            child.content,
            Content::Keybind {
                keybind: Cow::Owned("key.jump".into())
            }
        );
    }

    #[test]
    fn translate() {
        let root = parse("<lang:block.minecraft.diamond_block>");
        let child = first_child(&root);
        match &child.content {
            Content::Translate(msg) => {
                assert_eq!(msg.key, "block.minecraft.diamond_block");
                assert!(msg.fallback.is_none());
                assert!(msg.args.is_none());
            }
            _ => panic!("expected translation"),
        }
    }

    #[test]
    fn translate_with_args() {
        let root = parse("<lang:commands.drop.success.single:'<red>1':'<blue>Stone'>");
        let child = first_child(&root);
        match &child.content {
            Content::Translate(msg) => {
                assert_eq!(msg.key, "commands.drop.success.single");
                let args = msg.args.as_ref().unwrap();
                assert_eq!(args.len(), 2);
                let arg1 = &args[0];
                let red_child = arg1.children.first().unwrap();
                assert_eq!(red_child.format.color, Some(Color::Red));
                assert_eq!(
                    red_child.children[0].content,
                    Content::Text {
                        text: Cow::Borrowed("1")
                    }
                );
                let arg2 = &args[1];
                let blue_child = arg2.children.first().unwrap();
                assert_eq!(blue_child.format.color, Some(Color::Blue));
                assert_eq!(
                    blue_child.children[0].content,
                    Content::Text {
                        text: Cow::Borrowed("Stone")
                    }
                );
            }
            _ => panic!("expected translation"),
        }
    }

    #[test]
    fn translate_with_fallback() {
        let root = parse("<lang_or:my.key:Fallback>");
        let child = first_child(&root);
        match &child.content {
            Content::Translate(msg) => {
                assert_eq!(msg.key, "my.key");
                assert_eq!(msg.fallback, Some(Cow::Owned("Fallback".into())));
                assert!(msg.args.is_none());
            }
            _ => panic!("expected translation with fallback"),
        }
    }

    #[test]
    fn newline() {
        let root = parse("Line1<newline>Line2");
        let kids = children(&root);
        assert_eq!(kids.len(), 1);
        assert_eq!(
            kids[0].content,
            Content::Text {
                text: Cow::Borrowed("Line1\nLine2")
            }
        );
    }

    #[test]
    fn selector() {
        let root = parse("<sel:@a>");
        let child = first_child(&root);
        match &child.content {
            Content::Resolvable(Resolvable::Entity {
                selector,
                separator: _,
            }) => {
                assert_eq!(selector, "@a");
            }
            _ => panic!("expected entity selector"),
        }
    }

    #[test]
    fn selector_with_separator() {
        let root = parse("<sel:@a:', '>");
        let child = first_child(&root);
        match &child.content {
            Content::Resolvable(Resolvable::Entity {
                selector,
                separator,
            }) => {
                assert_eq!(selector, "@a");
                let sep_text = separator.children.first().unwrap();
                assert_eq!(
                    sep_text.content,
                    Content::Text {
                        text: Cow::Borrowed(", ")
                    }
                );
            }
            _ => panic!("expected entity selector with separator"),
        }
    }

    #[test]
    fn score() {
        let root = parse("<score:player:deaths>");
        let child = first_child(&root);
        match &child.content {
            Content::Resolvable(Resolvable::Scoreboard {
                selector,
                objective,
            }) => {
                assert_eq!(selector, "player");
                assert_eq!(objective, "deaths");
            }
            _ => panic!("expected scoreboard"),
        }
    }

    #[test]
    fn nbt_entity() {
        let root = parse("<nbt:entity:@s:Health>");
        let child = first_child(&root);
        match &child.content {
            Content::Resolvable(Resolvable::NBT {
                path,
                source,
                interpret,
                separator: _,
            }) => {
                assert_eq!(path, "Health");
                assert_eq!(*source, NbtSource::Entity(Cow::Owned("@s".into())));
                assert!(interpret.is_none());
            }
            _ => panic!("expected nbt"),
        }
    }

    #[test]
    fn nbt_with_interpret() {
        let root = parse("<nbt:block:12 34 56:Items:, :interpret>");
        let child = first_child(&root);
        match &child.content {
            Content::Resolvable(Resolvable::NBT {
                source, interpret, ..
            }) => {
                assert!(*interpret == Some(true));
                assert_eq!(*source, NbtSource::Block(Cow::Owned("12 34 56".into())));
            }
            _ => panic!("expected nbt with interpret"),
        }
    }

    #[test]
    fn nbt_with_separator() {
        let root = parse("<nbt:storage:foo:bar:', ':interpret>");
        let child = first_child(&root);
        match &child.content {
            Content::Resolvable(Resolvable::NBT {
                separator,
                source,
                interpret,
                ..
            }) => {
                assert_eq!(*source, NbtSource::Storage(Cow::Owned("foo".into())));
                assert!(*interpret == Some(true));
                let sep_text = separator.children.first().unwrap();
                assert_eq!(
                    sep_text.content,
                    Content::Text {
                        text: Cow::Borrowed(", ")
                    }
                );
            }
            _ => panic!("expected nbt with separator"),
        }
    }

    #[test]
    fn sprite_full() {
        let root = parse("<sprite:blocks:item/diamond_sword>");
        let child = first_child(&root);
        match &child.content {
            Content::Object(Object::Atlas { atlas, sprite }) => {
                assert_eq!(atlas.as_deref(), Some("blocks"));
                assert_eq!(sprite, "item/diamond_sword");
            }
            _ => panic!("expected sprite"),
        }
    }

    #[test]
    fn sprite_only() {
        let root = parse("<sprite:item/emerald>");
        let child = first_child(&root);
        match &child.content {
            Content::Object(Object::Atlas { atlas, sprite }) => {
                assert!(atlas.is_none());
                assert_eq!(sprite, "item/emerald");
            }
            _ => panic!("expected sprite"),
        }
    }

    #[test]
    fn head_by_name() {
        let root = parse("<head:Strokkur24>");
        let child = first_child(&root);
        match &child.content {
            Content::Object(Object::Player { player, hat }) => {
                assert!(hat);
                assert_eq!(player.name, Some("Strokkur24".into()));
            }
            _ => panic!("expected player head"),
        }
    }

    #[test]
    fn head_no_outer_layer() {
        let root = parse("<head:Strokkur24:false>");
        let child = first_child(&root);
        match &child.content {
            Content::Object(Object::Player { player: _, hat }) => assert!(!hat),
            _ => panic!("expected head"),
        }
    }

    #[test]
    fn head_by_uuid() {
        let uuid_str = "1f085b2d-9548-4159-a8c7-f3ccdf0c2054";
        let root = parse(&format!("<head:{}>", uuid_str));
        let child = first_child(&root);
        assert!(matches!(
            child.content,
            Content::Object(Object::Player { .. })
        ));
    }

    #[cfg(feature = "custom")]
    #[test]
    fn rainbow() {
        let root = parse("<rainbow>hello</rainbow>");
        let child = first_child(&root);
        match &child.content {
            Content::Custom(data) => assert_eq!(data.id, "rainbow"),
            _ => panic!("expected rainbow custom element"),
        }
    }

    #[cfg(feature = "custom")]
    #[test]
    fn gradient() {
        let root = parse("<gradient>hello</gradient>");
        let child = first_child(&root);
        match &child.content {
            Content::Custom(data) => assert_eq!(data.id, "gradient"),
            _ => panic!("expected gradient"),
        }
    }

    #[cfg(feature = "custom")]
    #[test]
    fn transition() {
        let root = parse("<transition>hello</transition>");
        let child = first_child(&root);
        match &child.content {
            Content::Custom(data) => assert_eq!(data.id, "transition"),
            _ => panic!("expected transition"),
        }
    }

    #[cfg(feature = "custom")]
    #[test]
    fn pride() {
        let root = parse("<pride>hello</pride>");
        let child = first_child(&root);
        match &child.content {
            Content::Custom(data) => assert_eq!(data.id, "pride"),
            _ => panic!("expected pride"),
        }
    }

    #[test]
    fn self_closing_tag() {
        let root = parse("<yellow/>Hello");
        let kids = children(&root);
        assert_eq!(kids.len(), 2);
        assert_eq!(kids[0].format.color, Some(Color::Yellow));
        assert_eq!(
            kids[0].content,
            Content::Text {
                text: Cow::Borrowed("")
            }
        );
        assert_eq!(
            kids[1].content,
            Content::Text {
                text: Cow::Borrowed("Hello")
            }
        );
    }

    #[test]
    fn unclosed_tag() {
        let root = parse("<yellow>Hello");
        let child = first_child(&root);
        assert_eq!(child.format.color, Some(Color::Yellow));
        assert_eq!(
            child.children[0].content,
            Content::Text {
                text: Cow::Borrowed("Hello")
            }
        );
    }

    #[test]
    fn escape_backslash() {
        let root = parse(r"\\<red>test");
        let kids = children(&root);
        assert_eq!(kids.len(), 2);
        assert_eq!(
            kids[0].content,
            Content::Text {
                text: Cow::Owned("\\".into())
            }
        );
        let red_wrapper = &kids[1];
        assert_eq!(red_wrapper.format.color, Some(Color::Red));
        assert_eq!(red_wrapper.children.len(), 1);
        assert_eq!(
            red_wrapper.children[0].content,
            Content::Text {
                text: Cow::Borrowed("test")
            }
        );
    }

    #[test]
    fn unknown_tag_ignored() {
        let root = parse("<unknown>test</unknown>");
        let child = first_child(&root);
        assert_eq!(
            child.content,
            Content::Text {
                text: Cow::Owned("test".into())
            }
        );
    }

    #[test]
    fn mixed_formatting() {
        let root = parse("<bold><italic>Text</italic></bold>");
        let bold = first_child(&root);
        assert_eq!(bold.format.bold, Some(true));
        let italic = &bold.children[0];
        assert_eq!(italic.format.italic, Some(true));
        let text = &italic.children[0];
        assert_eq!(
            text.content,
            Content::Text {
                text: Cow::Borrowed("Text")
            }
        );
    }

    #[test]
    fn quoted_args_with_escaped_quote() {
        let root = parse(r"<hover:show_text:'It\'s a test'>Hover");
        let child = first_child(&root);
        match &child.interactions.hover {
            Some(HoverEvent::ShowText { value }) => {
                let inner_child = value.children.first().unwrap();
                assert_eq!(
                    inner_child.content,
                    Content::Text {
                        text: Cow::Owned("It's a test".into())
                    }
                );
            }
            _ => panic!("expected hover"),
        }
    }
}
