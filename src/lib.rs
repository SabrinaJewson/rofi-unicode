use ::{
    anyhow::Context as _,
    rofi_mode::Mode,
    std::{io::Write, process},
};

struct Entry {
    data: &'static str,
    complete_with: &'static str,
    displayed: &'static str,
    displayed_no_markup: &'static str,
}

#[rustfmt::skip]
mod generated;

rofi_mode::export_mode!(Unicode);

struct Unicode {}

impl Unicode {
    fn try_init() -> anyhow::Result<Self> {
        Ok(Self {})
    }
}

impl Mode<'_> for Unicode {
    const NAME: &'static str = "unicode-selector\0";
    const DISPLAY_NAME: &'static str = "unicode\0";

    fn init(_api: rofi_mode::Api<'_>) -> Result<Self, ()> {
        Self::try_init().map_err(|e| eprintln!("Error: {e:?}"))
    }

    fn entries(&mut self) -> usize {
        generated::ENTRIES.len()
    }

    fn entry_style(&self, _line: usize) -> rofi_mode::Style {
        rofi_mode::Style::MARKUP
    }

    fn entry_content(&self, line: usize) -> rofi_mode::String {
        rofi_mode::String::from(generated::ENTRIES[line].displayed)
    }

    fn completed(&self, line: usize) -> rofi_mode::String {
        rofi_mode::String::from(generated::ENTRIES[line].displayed_no_markup)
    }

    fn react(
        &mut self,
        event: rofi_mode::Event,
        input: &mut rofi_mode::String,
    ) -> rofi_mode::Action {
        match event {
            rofi_mode::Event::Cancel { .. } => rofi_mode::Action::Exit,
            rofi_mode::Event::Ok { selected, .. } => {
                let data = generated::ENTRIES[selected].data;

                if let Err(e) = clipboard_copy(data) {
                    eprintln!("failed to copy text to clipboard: {e:?}");
                    return rofi_mode::Action::Reload;
                }

                rofi_mode::Action::Exit
            }
            rofi_mode::Event::Complete {
                selected: Some(selected),
            } => {
                input.clear();
                input.push_str(generated::ENTRIES[selected].complete_with);
                rofi_mode::Action::Reload
            }
            rofi_mode::Event::CustomInput { .. }
            | rofi_mode::Event::Complete { selected: None }
            | rofi_mode::Event::DeleteEntry { .. }
            | rofi_mode::Event::CustomCommand { .. } => rofi_mode::Action::Reload,
        }
    }

    fn matches(&self, line: usize, matcher: rofi_mode::Matcher<'_>) -> bool {
        matcher.matches(generated::ENTRIES[line].displayed)
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
