use std::path::PathBuf;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
pub enum Error {
    Io(std::io::Error),
    Image(image::ImageError),
    Zip(zip::result::ZipError),
    Json(serde_json::Error),
    MissingFile(String),
    MissingData(String),
    InvalidTemplate(String),
    FileNotFound(PathBuf),
    NoRenders,
    NoMapping,
    NoRepository,
    NoInputs,
    GifEncode(String),
    VideoEncode(String),
    TextRender(String),
    Other(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Io(e) => write!(f, "IO error: {}", e),
            Error::Image(e) => write!(f, "Image error: {}", e),
            Error::Zip(e) => write!(f, "Zip error: {}", e),
            Error::Json(e) => write!(f, "JSON error: {}", e),
            Error::MissingFile(name) => write!(f, "Missing file: {}", name),
            Error::MissingData(msg) => write!(f, "Missing data: {}", msg),
            Error::InvalidTemplate(msg) => write!(f, "Invalid template: {}", msg),
            Error::FileNotFound(path) => write!(f, "File not found: {}", path.display()),
            Error::NoRenders => write!(f, "No renders attached"),
            Error::NoMapping => write!(f, "No mapping attached"),
            Error::NoRepository => write!(f, "No repository attached"),
            Error::NoInputs => write!(f, "No inputs attached"),
            Error::GifEncode(msg) => write!(f, "GIF encoding error: {}", msg),
            Error::VideoEncode(msg) => write!(f, "Video encoding error: {}", msg),
            Error::TextRender(msg) => write!(f, "Text rendering error: {}", msg),
            Error::Other(msg) => write!(f, "{}", msg),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Io(e) => Some(e),
            Error::Image(e) => Some(e),
            Error::Zip(e) => Some(e),
            Error::Json(e) => Some(e),
            _ => None,
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}

impl From<image::ImageError> for Error {
    fn from(e: image::ImageError) -> Self {
        Error::Image(e)
    }
}

impl From<zip::result::ZipError> for Error {
    fn from(e: zip::result::ZipError) -> Self {
        Error::Zip(e)
    }
}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Error::Json(e)
    }
}

impl From<String> for Error {
    fn from(s: String) -> Self {
        Error::Other(s)
    }
}

impl From<&str> for Error {
    fn from(s: &str) -> Self {
        Error::Other(s.to_string())
    }
}
