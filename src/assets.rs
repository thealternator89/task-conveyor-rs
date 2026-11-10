use gpui::{AssetSource, Result, SharedString};
use rust_embed::RustEmbed;
use std::borrow::Cow;

#[derive(RustEmbed)]
#[folder = "assets"]
pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        Ok(Self::get(path).map(|f| f.data))
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        Ok(Self::iter()
            .filter(|p| p.starts_with(path))
            .map(SharedString::from)
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_embedded_assets_present() {
        let assets = Assets;
        for expected in [
            "icons/terminal.svg",
            "icons/thumbtack.svg",
            "logo-full.png",
            "icon.icns",
            "icon.ico",
        ] {
            let loaded = assets.load(expected).unwrap();
            assert!(
                loaded.is_some(),
                "Expected embedded asset '{expected}' to exist"
            );
            assert!(
                !loaded.unwrap().is_empty(),
                "Embedded asset '{expected}' should not be empty"
            );
        }

        let icons = assets.list("icons/").unwrap();
        assert!(icons.iter().any(|p| p.as_ref() == "icons/terminal.svg"));
        assert!(icons.iter().any(|p| p.as_ref() == "icons/thumbtack.svg"));
    }
}
