use {
    super::{with_glib_markup_escaped, Content, Item, Items, Opts},
    ::{anyhow::Context as _, std::collections::HashMap},
};

pub(super) fn generate(opts: &Opts<'_>) -> anyhow::Result<()> {
    const EMOJI_LIST: &str = "emoji/charts/emoji-list.txt";
    let emojis_txt = opts.load_text_unicode(EMOJI_LIST)?;
    let emojis = parse_emoji_list(&*emojis_txt)
        .map(|res| res.with_context(|| format!("failed to parse {EMOJI_LIST}")));

    const EMOJI_MODIFIERS: &str = "emoji/charts/full-emoji-modifiers.txt";
    let modifiers_txt = opts.load_text_unicode(EMOJI_MODIFIERS)?;
    let modifiers = parse_emoji_list(&*modifiers_txt)
        .map(|res| res.with_context(|| format!("failed to parse {EMOJI_MODIFIERS}")));

    const EMOJI_ORDERING: &str = "emoji/charts/emoji-ordering.txt";
    let ordering_txt = opts.load_text_unicode(EMOJI_ORDERING)?;
    let ordering = parse_emoji_order(&*ordering_txt)
        .map(|res| res.with_context(|| format!("failed to parse {EMOJI_ORDERING}")));

    let mut all_emojis = Iterator::chain(
        emojis.map(|res| res.map(|(value, description)| (value, (description, false)))),
        modifiers.map(|res| res.map(|(value, description)| (value, (description, true)))),
    )
    .collect::<anyhow::Result<HashMap<_, _>>>()?;

    let ordered = ordering
        .map(|res| {
            let emoji = res?;
            let (description, variation) = all_emojis
                .remove(&*emoji)
                .with_context(|| format!("emoji {} found in ordering but not in list", emoji))?;

            let name = with_glib_markup_escaped(description, |s| s.to_owned());

            let item = Item {
                name: format!("{emoji}\t{name}"),
                content: Content::Text(emoji),
            };

            Ok((item, variation))
        })
        .collect::<anyhow::Result<Vec<(Item, bool)>>>()?;

    anyhow::ensure!(all_emojis.is_empty(), "emojis in list but not ordering");

    let mut items = Vec::new();

    let mut ordered = ordered.into_iter().peekable();
    while let Some((base, is_variation)) = ordered.next() {
        assert!(!is_variation);

        // If there are 1+ variations of this emoji, put them in a list.
        let item = if ordered.peek().map_or(false, |(_, variation)| *variation) {
            let mut variations = vec![base];
            while let Some((item, _)) = ordered.next_if(|(_, variation)| *variation) {
                variations.push(item);
            }
            Item {
                name: variations[0].name.clone(),
                content: Content::Items(Items::from_direct(variations)),
            }
        } else {
            base
        };
        items.push(item);
    }

    opts.write_ron("emojis.ron", Items::from_direct(items))?;

    Ok(())
}

fn parse_emoji_list(file: &str) -> impl '_ + Iterator<Item = anyhow::Result<(String, &str)>> {
    file.lines()
        .enumerate()
        .filter(|(_, line)| !line.starts_with('@'))
        .map(|(i, line)| {
            parse_emoji_line(line)
                .with_context(|| format!("error parsing emoji list line {}", i + 1))
        })
}

fn parse_emoji_line(line: &str) -> anyhow::Result<(String, &str)> {
    let (codepoints, description) = line.split_once('\t').context("line does not contain tab")?;

    let value = codepoints
        .split(' ')
        .map(parse_scalar_value)
        .collect::<anyhow::Result<String>>()?;

    Ok((value, description))
}

fn parse_emoji_order(file: &str) -> impl '_ + Iterator<Item = anyhow::Result<String>> {
    file.lines().enumerate().filter_map(|(i, line)| {
        parse_emoji_order_line(line)
            .with_context(|| format!("error parsing emoji order file line {}", i + 1))
            .transpose()
    })
}

fn parse_emoji_order_line(line: &str) -> anyhow::Result<Option<String>> {
    let no_comment = line.split('#').next().unwrap();
    if no_comment.is_empty() {
        return Ok(None);
    }
    let first_field = no_comment.split(';').next().unwrap();
    let emoji = first_field
        .trim()
        .split_whitespace()
        .map(|prefixed| {
            let codepoint = prefixed
                .strip_prefix("U+")
                .with_context(|| format!("codepoint {prefixed} does not start with U+"))?;
            parse_scalar_value(codepoint)
        })
        .collect::<anyhow::Result<String>>()?;

    anyhow::ensure!(!emoji.is_empty(), "found empty emoji");

    Ok(Some(emoji))
}

fn parse_scalar_value(codepoint: &str) -> anyhow::Result<char> {
    let codepoint = u32::from_str_radix(codepoint, 16)
        .with_context(|| format!("{codepoint} is not a valid code point"))?;
    let scalar_value =
        char::from_u32(codepoint).with_context(|| format!("{codepoint:X} is a surrogate"))?;
    Ok(scalar_value)
}
