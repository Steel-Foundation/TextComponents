use crate::{
    TextComponent,
    content::{Content, NbtSource, Object, ObjectPlayer, PlayerProperties, Resolvable},
    custom::CustomData,
    format::{Color, Format},
    interactivity::{ClickEvent, HoverEvent, Interactivity},
    translation::TranslatedMessage,
};

use simdnbt::owned::{NbtCompound, NbtList, NbtTag};
use uuid::Uuid;

use std::borrow::Cow;

impl TextComponent {
    pub fn from_nbt(tag: &NbtTag) -> Option<Self> {
        match tag {
            NbtTag::String(string) => {
                if string.is_empty() {
                    return None;
                }
                Some(TextComponent::plain(string.to_string()))
            }
            NbtTag::Compound(compound) => {
                if let Some(tag) = compound.get("") {
                    match tag {
                        NbtTag::String(..) | NbtTag::List(..) => {
                            return TextComponent::from_nbt(tag);
                        }
                        _ => (),
                    }
                }
                let mut children = vec![];
                if let Some(tag) = compound.get("extra")
                    && let NbtTag::List(list) = tag
                {
                    for child in list.as_nbt_tags() {
                        let child = TextComponent::from_nbt(&child);
                        if let Some(child) = child {
                            children.push(child);
                        }
                    }
                }
                Some(TextComponent {
                    content: Content::from_compound(compound)?,
                    children,
                    format: Format::from_compound(compound),
                    interactions: Interactivity::from_compound(compound),
                })
            }
            _ => None,
        }
    }
}

impl Content {
    fn from_compound(compound: &NbtCompound) -> Option<Self> {
        if let Some(tag) = compound.get("text")
            && let NbtTag::String(text) = tag
        {
            return Some(Content::Text {
                text: text.to_string().into(),
            });
        }
        if let Some(tag) = compound.get("translate")
            && let NbtTag::String(key) = tag
        {
            let mut fallback = None;
            let mut args = None;
            if let Some(tag) = compound.get("fallback")
                && let NbtTag::String(text) = tag
            {
                fallback = Some(Cow::Owned(text.to_string()));
            }
            if let Some(tag) = compound.get("with")
                && let NbtTag::List(list) = tag
            {
                let mut args_vec = vec![];
                for arg in list.as_nbt_tags() {
                    if let Some(arg) = TextComponent::from_nbt(&arg) {
                        args_vec.push(arg);
                    }
                }
                args = Some(args_vec.into_boxed_slice());
            }

            return Some(Content::Translate(TranslatedMessage {
                key: key.to_string().into(),
                fallback,
                args,
            }));
        }
        if let Some(tag) = compound.get("keybind")
            && let NbtTag::String(key) = tag
        {
            return Some(Content::Keybind {
                keybind: key.to_string().into(),
            });
        }
        if let Some(tag) = compound.get("score")
            && let NbtTag::Compound(compound) = tag
        {
            let NbtTag::String(selector) = compound.get("name")? else {
                return None;
            };
            let NbtTag::String(objective) = compound.get("objective")? else {
                return None;
            };
            return Some(Content::Resolvable(Resolvable::Scoreboard {
                selector: selector.to_string().into(),
                objective: objective.to_string().into(),
            }));
        }
        if let Some(tag) = compound.get("selector")
            && let NbtTag::String(selector) = tag
        {
            let mut separator = Resolvable::entity_separator();
            if let Some(tag) = compound.get("separator")
                && let Some(component) = TextComponent::from_nbt(tag)
            {
                *separator = component;
            };
            return Some(Content::Resolvable(Resolvable::Entity {
                selector: selector.to_string().into(),
                separator,
            }));
        }
        if let Some(tag) = compound.get("nbt")
            && let NbtTag::String(path) = tag
        {
            let mut interpret = None;
            let mut separator = Resolvable::entity_separator();
            let mut source = NbtSource::Block(Cow::Borrowed(""));
            let mut continues = true;
            if let Some(tag) = compound.get("interpret") {
                match tag {
                    NbtTag::String(str) => {
                        if str.to_str() == "true" {
                            interpret = Some(true);
                        } else if str.to_str() == "false" {
                            interpret = Some(false);
                        }
                    }
                    NbtTag::Byte(val) => interpret = Some(*val != 0),
                    _ => (),
                }
            }
            if let Some(tag) = compound.get("separator")
                && let Some(component) = TextComponent::from_nbt(tag)
            {
                *separator = component;
            };
            if let Some(tag) = compound.get("source")
                && let NbtTag::String(s_type) = tag
            {
                continues = false;
                match &*s_type.to_str() {
                    "block" => {
                        if let Some(tag) = compound.get("block")
                            && let NbtTag::String(text) = tag
                        {
                            source = NbtSource::Block(Cow::Owned(text.to_string()))
                        }
                    }
                    "entity" => {
                        if let Some(tag) = compound.get("entity")
                            && let NbtTag::String(text) = tag
                        {
                            source = NbtSource::Entity(Cow::Owned(text.to_string()))
                        }
                    }
                    "storage" => {
                        if let Some(tag) = compound.get("storage")
                            && let NbtTag::String(text) = tag
                        {
                            source = NbtSource::Storage(Cow::Owned(text.to_string()))
                        }
                    }
                    _ => continues = true,
                }
            }
            if continues
                && let Some(tag) = compound.get("block")
                && let NbtTag::String(text) = tag
            {
                source = NbtSource::Block(Cow::Owned(text.to_string()))
            } else if continues
                && let Some(tag) = compound.get("entity")
                && let NbtTag::String(text) = tag
            {
                source = NbtSource::Entity(Cow::Owned(text.to_string()))
            } else if continues
                && let Some(tag) = compound.get("storage")
                && let NbtTag::String(text) = tag
            {
                source = NbtSource::Storage(Cow::Owned(text.to_string()))
            }
            return Some(Content::Resolvable(Resolvable::NBT {
                path: path.to_string().into(),
                interpret,
                separator,
                source,
            }));
        }
        if let Some(tag) = compound.get("sprite")
            && let NbtTag::String(sprite) = tag
        {
            let mut atlas = None;
            if let Some(tag) = compound.get("atlas")
                && let NbtTag::String(text) = tag
            {
                atlas = Some(text.to_string().into())
            }

            return Some(Content::Object(Object::Atlas {
                atlas,
                sprite: sprite.to_string().into(),
            }));
        }
        if let Some(tag) = compound.get("object")
            && let NbtTag::String(obj_type) = tag
        {
            match &*obj_type.to_str() {
                "player" => {
                    let mut player = ObjectPlayer {
                        name: None,
                        id: None,
                        texture: None,
                        properties: vec![],
                    };
                    let mut hat = true;
                    if let Some(tag) = compound.get("player")
                        && let NbtTag::Compound(compound) = tag
                    {
                        if let Some(tag) = compound.get("name")
                            && let NbtTag::String(name) = tag
                        {
                            player.name = Some(Cow::Owned(name.to_string()))
                        }
                        if let Some(tag) = compound.get("id")
                            && let NbtTag::IntArray(nums) = tag
                            && nums.len() == 4
                        {
                            player.id = Some([nums[0], nums[1], nums[2], nums[3]])
                        }
                        if let Some(tag) = compound.get("texture")
                            && let NbtTag::String(texture) = tag
                        {
                            player.texture = Some(Cow::Owned(texture.to_string()))
                        }
                        if let Some(tag) = compound.get("properties")
                            && let NbtTag::List(NbtList::Compound(compounds)) = tag
                        {
                            for compound in compounds {
                                if let Some(name_tag) = compound.get("name")
                                    && let Some(value_tag) = compound.get("value")
                                    && let NbtTag::String(name) = name_tag
                                    && let NbtTag::String(value) = value_tag
                                {
                                    let mut signature = None;
                                    if let Some(tag) = compound.get("signature")
                                        && let NbtTag::String(text) = tag
                                    {
                                        signature = Some(text.to_string().into())
                                    }
                                    player.properties.push(PlayerProperties {
                                        name: name.to_string().into(),
                                        value: value.to_string().into(),
                                        signature,
                                    });
                                }
                            }
                        }
                    }
                    if let Some(tag) = compound.get("hat") {
                        match tag {
                            NbtTag::String(str) if str.to_str() == "false" => {
                                hat = false;
                            }
                            NbtTag::Byte(val) => hat = *val != 0,
                            _ => (),
                        }
                    }
                    return Some(Content::Object(Object::Player { player, hat }));
                }
                _ => return None,
            }
        }
        #[cfg(feature = "custom")]
        if let Some(tag) = compound.get("custom")
            && let NbtTag::Compound(compound) = tag
        {
            return Some(Content::Custom(CustomData::from_compound(compound)?));
        }

        None
    }
}

impl Format {
    fn from_compound(compound: &NbtCompound) -> Self {
        let mut format = Format::new();
        if let Some(tag) = compound.get("color")
            && let NbtTag::String(color) = tag
        {
            let color = color.to_string();
            if color.starts_with("#") {
                format = format.color_hex(&color);
            }
            match color.as_str() {
                "aqua" => format = format.color(Color::Aqua),
                "black" => format = format.color(Color::Black),
                "blue" => format = format.color(Color::Blue),
                "dark_aqua" => format = format.color(Color::DarkAqua),
                "dark_blue" => format = format.color(Color::DarkBlue),
                "dark_gray" => format = format.color(Color::DarkGray),
                "dark_green" => format = format.color(Color::DarkGreen),
                "dark_purple" => format = format.color(Color::DarkPurple),
                "dark_red" => format = format.color(Color::DarkRed),
                "gold" => format = format.color(Color::Gold),
                "gray" => format = format.color(Color::Gray),
                "green" => format = format.color(Color::Green),
                "light_purple" => format = format.color(Color::LightPurple),
                "red" => format = format.color(Color::Red),
                "white" => format = format.color(Color::White),
                "yellow" => format = format.color(Color::Yellow),
                _ => (),
            }
        }
        if let Some(tag) = compound.get("font")
            && let NbtTag::String(color) = tag
        {
            format = format.font(color.to_string());
        }
        if let Some(tag) = compound.get("bold") {
            match tag {
                NbtTag::String(str) => {
                    if str.to_str() == "true" {
                        format = format.bold(true);
                    } else if str.to_str() == "false" {
                        format = format.bold(false);
                    }
                }
                NbtTag::Byte(val) => format = format.bold(*val != 0),
                _ => (),
            }
        }
        if let Some(tag) = compound.get("italic") {
            match tag {
                NbtTag::String(str) => {
                    if str.to_str() == "true" {
                        format = format.italic(true);
                    } else if str.to_str() == "false" {
                        format = format.italic(false);
                    }
                }
                NbtTag::Byte(val) => format = format.italic(*val != 0),
                _ => (),
            }
        }
        if let Some(tag) = compound.get("underlined") {
            match tag {
                NbtTag::String(str) => {
                    if str.to_str() == "true" {
                        format = format.underlined(true);
                    } else if str.to_str() == "false" {
                        format = format.underlined(false);
                    }
                }
                NbtTag::Byte(val) => format = format.underlined(*val != 0),
                _ => (),
            }
        }
        if let Some(tag) = compound.get("strikethrough") {
            match tag {
                NbtTag::String(str) => {
                    if str.to_str() == "true" {
                        format = format.strikethrough(true);
                    } else if str.to_str() == "false" {
                        format = format.strikethrough(false);
                    }
                }
                NbtTag::Byte(val) => format = format.strikethrough(*val != 0),
                _ => (),
            }
        }
        if let Some(tag) = compound.get("obfuscated") {
            match tag {
                NbtTag::String(str) => {
                    if str.to_str() == "true" {
                        format = format.obfuscated(true);
                    } else if str.to_str() == "false" {
                        format = format.obfuscated(false);
                    }
                }
                NbtTag::Byte(val) => format = format.obfuscated(*val != 0),
                _ => (),
            }
        }
        if let Some(tag) = compound.get("shadow_color") {
            match tag {
                NbtTag::Short(n) => format.shadow_color = Some(*n as i64),
                NbtTag::Int(n) => format.shadow_color = Some(*n as i64),
                NbtTag::Long(n) => format.shadow_color = Some(*n),
                NbtTag::List(list) => {
                    let list = list.as_nbt_tags();
                    if list.len() == 4 {
                        let mut nums = vec![];
                        for item in list {
                            match item {
                                NbtTag::Float(n) => nums.push((n * 255.) as u8),
                                NbtTag::Double(n) => nums.push((n * 255.) as u8),
                                _ => break,
                            }
                        }
                        if nums.len() == 4 {
                            format = format.shadow_color(nums[3], nums[0], nums[1], nums[2]);
                        }
                    }
                }
                _ => (),
            }
        }
        format
    }
}

impl Interactivity {
    fn from_compound(compound: &NbtCompound) -> Self {
        let mut interaction = Interactivity::new();
        if let Some(tag) = compound.get("insertion")
            && let NbtTag::String(insertion) = tag
        {
            interaction.insertion = Some(Cow::Owned(insertion.to_string()));
        }

        if let Some(tag) = compound.get("click_event")
            && let NbtTag::Compound(event) = tag
        {
            interaction.click = ClickEvent::from_compound(event);
        }
        if let Some(tag) = compound.get("hover_event")
            && let NbtTag::Compound(event) = tag
        {
            interaction.hover = HoverEvent::from_compound(event);
        }
        interaction
    }
}

impl HoverEvent {
    fn from_compound(compound: &NbtCompound) -> Option<Self> {
        let tag = compound.get("action")?;
        if let NbtTag::String(event) = tag {
            return match &*event.to_str() {
                "show_text" => {
                    let value = compound.get("value")?;
                    let compound = TextComponent::from_nbt(value)?;
                    Some(HoverEvent::ShowText {
                        value: Box::new(compound),
                    })
                }
                "show_item" => {
                    let id = compound.get("id")?;
                    match id {
                        NbtTag::String(id) => {
                            let mut count = None;
                            let mut components = None;
                            if let Some(tag) = compound.get("count")
                                && let NbtTag::Int(n) = tag
                            {
                                count = Some(*n);
                            }
                            if let Some(tag) = compound.get("components")
                                && let NbtTag::Compound(comps) = tag
                            {
                                let mut data = vec![];
                                comps.write(&mut data);
                                if let Ok(comps) = String::from_utf8(data) {
                                    components = Some(comps.into());
                                }
                            }
                            Some(HoverEvent::ShowItem {
                                id: id.to_string().into(),
                                count,
                                components,
                            })
                        }
                        _ => None,
                    }
                }
                "show_entity" => {
                    let id = compound.get("id")?;
                    let uuid = compound.get("uuid")?;
                    let uuid: Uuid = match uuid {
                        NbtTag::String(uuid) => Uuid::parse_str(&uuid.to_string()).ok()?,
                        NbtTag::IntArray(nums) | NbtTag::List(NbtList::Int(nums)) => {
                            if nums.len() != 4 {
                                return None;
                            }
                            Uuid::from_u64_pair(
                                (((nums[0] as u32) as u64) << 32) + ((nums[1] as u32) as u64),
                                (((nums[2] as u32) as u64) << 32) + ((nums[3] as u32) as u64),
                            )
                        }
                        _ => return None,
                    };
                    match id {
                        NbtTag::String(id) => {
                            let mut name = None;
                            if let Some(name_nbt) = compound.get("name")
                                && let Some(compound) = TextComponent::from_nbt(name_nbt)
                            {
                                name = Some(Box::new(compound))
                            }
                            Some(HoverEvent::ShowEntity {
                                name,
                                id: id.to_string().into(),
                                uuid,
                            })
                        }
                        _ => None,
                    }
                }
                _ => None,
            };
        }
        None
    }
}

impl ClickEvent {
    fn from_compound(compound: &NbtCompound) -> Option<Self> {
        let tag = compound.get("action")?;
        if let NbtTag::String(event) = tag {
            return match &*event.to_str() {
                "open_url" => {
                    if let Some(tag) = compound.get("url")
                        && let NbtTag::String(url) = tag
                    {
                        return Some(ClickEvent::OpenUrl {
                            url: url.to_string().into(),
                        });
                    }
                    None
                }
                "run_command" => {
                    if let Some(tag) = compound.get("command")
                        && let NbtTag::String(command) = tag
                    {
                        return Some(ClickEvent::RunCommand {
                            command: command.to_string().into(),
                        });
                    }
                    None
                }
                "suggest_command" => {
                    if let Some(tag) = compound.get("command")
                        && let NbtTag::String(command) = tag
                    {
                        return Some(ClickEvent::SuggestCommand {
                            command: command.to_string().into(),
                        });
                    }
                    None
                }
                "change_page" => {
                    if let Some(tag) = compound.get("page")
                        && let NbtTag::Int(page) = tag
                    {
                        return Some(ClickEvent::ChangePage { page: *page });
                    }
                    None
                }
                "copy_to_clipboard" => {
                    if let Some(tag) = compound.get("value")
                        && let NbtTag::String(value) = tag
                    {
                        return Some(ClickEvent::CopyToClipboard {
                            value: value.to_string().into(),
                        });
                    }
                    None
                }
                "show_dialog" => {
                    if let Some(tag) = compound.get("dialog")
                        && let NbtTag::String(dialog) = tag
                    {
                        return Some(ClickEvent::ShowDialog {
                            dialog: dialog.to_string().into(),
                        });
                    }
                    None
                }
                #[cfg(feature = "custom")]
                "custom" => Some(ClickEvent::Custom(CustomData::from_compound(compound)?)),
                _ => None,
            };
        }
        None
    }
}

#[cfg(feature = "custom")]
impl CustomData {
    fn from_compound(compound: &NbtCompound) -> Option<Self> {
        use crate::custom::{CustomData, Payload};

        let tag = compound.get("id")?;
        if let NbtTag::String(id) = tag {
            return Some(CustomData {
                id: id.to_string().into(),
                // TODO: End payload serialization
                payload: Payload::Empty,
            });
        }
        None
    }
}
