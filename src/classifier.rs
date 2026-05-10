pub fn categorize(ext: &str) -> &'static str {
    match ext {
        "mp4" | "mov" | "mkv" | "avi" | "webm"           => "video",
        "jpg" | "jpeg" | "png" | "gif" | "webp" | "heic" => "images",
        "zip" | "tar" | "gz" | "rar" | "7z" | "dmg"      => "archives",
        "mp3" | "flac" | "wav" | "aac" | "m4a"           => "audio",
        "pdf" | "doc" | "docx" | "pages" | "epub"        => "documents",
        "rs" | "py" | "js" | "ts" | "go" | "swift"       => "code",
        "log" | "tmp" | "cache" | "bak"                  => "junk",
        "none"                                           => "no extension",
        _                                                => "other",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_extensions_map_to_categories() {
        assert_eq!(categorize("mp4"), "video");
        assert_eq!(categorize("png"), "images");
        assert_eq!(categorize("rs"), "code");
        assert_eq!(categorize("none"), "no extension");
    }

    #[test]
    fn unknown_extension_is_other() {
        assert_eq!(categorize("xyz"), "other");
    }
}
