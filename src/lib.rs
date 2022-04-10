use ::{
    anyhow::Context as _,
    rofi_mode::Mode,
    std::{io::Write, process},
};

mod config;

rofi_mode::export_mode!(Unicode);

struct Unicode {
    lists: Vec<List>,
    active_list: usize,
}

impl Unicode {
    fn active_list(&self) -> &List {
        &self.lists[self.active_list]
    }
    fn item(&self, i: usize) -> &Item {
        &self.active_list().items[i]
    }
}

struct List {
    parent: Option<usize>,
    items: Box<[Item]>,
}

struct Item {
    name: String,
    name_attributes: Vec<pango::Attribute>,
    content: Content,
}

enum Content {
    Text(String),
    List(usize),
}

impl Unicode {
    fn try_init() -> anyhow::Result<Self> {
        let config = config::read().context("failed to read configuration")?;

        let mut lists = Vec::new();
        let active_list = register_items(config.root, &mut lists, None);
        assert_eq!(active_list, 0);

        Ok(Self { lists, active_list })
    }
}

fn register_items(items: Vec<config::Item>, lists: &mut Vec<List>, parent: Option<usize>) -> usize {
    let list_index = lists.len();
    lists.push(List {
        parent,
        items: Box::new([]),
    });

    lists[list_index].items = items
        .into_iter()
        .map(|config_item| Item {
            name: config_item.name,
            name_attributes: config_item.name_attributes,
            content: match config_item.content {
                config::Content::Text(text) => Content::Text(text),
                config::Content::Items(nested) => {
                    let index = register_items(nested, lists, Some(list_index));
                    Content::List(index)
                }
            },
        })
        .collect();

    list_index
}

impl Mode<'_> for Unicode {
    const NAME: &'static str = "unicode-selector\0";
    const DISPLAY_NAME: &'static str = "unicode\0";

    fn init(_api: rofi_mode::Api<'_>) -> Result<Self, ()> {
        Self::try_init().map_err(|e| eprintln!("Error: {e:?}"))
    }

    fn entries(&mut self) -> usize {
        self.active_list().items.len()
    }

    fn entry_attributes(&self, line: usize) -> rofi_mode::Attributes {
        self.item(line).name_attributes.iter().cloned().collect()
    }

    fn entry_content(&self, line: usize) -> rofi_mode::String {
        rofi_mode::String::from(&*self.item(line).name)
    }

    fn react(
        &mut self,
        event: rofi_mode::Event,
        input: &mut rofi_mode::String,
    ) -> rofi_mode::Action {
        match event {
            rofi_mode::Event::Cancel { .. } => {
                if let Some(parent) = self.active_list().parent {
                    self.active_list = parent;
                    rofi_mode::Action::Reload
                } else {
                    rofi_mode::Action::Exit
                }
            }
            rofi_mode::Event::Ok { selected, .. } => match &self.item(selected).content {
                Content::Text(data) => {
                    if let Err(e) = clipboard_copy(&*data) {
                        eprintln!("failed to copy text to clipboard: {e:?}");
                        return rofi_mode::Action::Reload;
                    }
                    rofi_mode::Action::Exit
                }
                Content::List(index) => {
                    self.active_list = *index;
                    rofi_mode::Action::Reload
                }
            },
            rofi_mode::Event::Complete {
                selected: Some(selected),
            } => {
                input.clear();
                input.push_str(&*self.item(selected).name);
                rofi_mode::Action::Reload
            }
            rofi_mode::Event::CustomInput { .. }
            | rofi_mode::Event::Complete { selected: None }
            | rofi_mode::Event::DeleteEntry { .. }
            | rofi_mode::Event::CustomCommand { .. } => rofi_mode::Action::Reload,
        }
    }

    fn matches(&self, line: usize, matcher: rofi_mode::Matcher<'_>) -> bool {
        matcher.matches(&*self.item(line).name)
    }
}

fn clipboard_copy(text: &str) -> anyhow::Result<()> {
    let mut child = process::Command::new("xclip")
        .arg("-selection")
        .arg("clipboard")
        .arg("-quiet")
        .current_dir("/")
        .stdin(process::Stdio::piped())
        .stdout(process::Stdio::null())
        .stderr(process::Stdio::null())
        .spawn()
        .context("failed to spawn xclip")?;
    child.stdin.take().unwrap().write_all(text.as_bytes())?;
    Ok(())
}
