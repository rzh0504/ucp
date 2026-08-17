use crate::model::AppLanguage;
use chrono::{DateTime, Local};

pub struct Translations {
    pub image: &'static str,
}

const CHINESE: Translations = Translations { image: "图片" };

const ENGLISH: Translations = Translations { image: "Image" };

pub fn tr(language: AppLanguage) -> &'static Translations {
    match language {
        AppLanguage::Chinese => &CHINESE,
        AppLanguage::English => &ENGLISH,
    }
}

pub fn character_count(language: AppLanguage, count: usize) -> String {
    match language {
        AppLanguage::Chinese => format!("{count} 字符"),
        AppLanguage::English => format!("{count} characters"),
    }
}

pub fn file_count(language: AppLanguage, count: usize) -> String {
    match language {
        AppLanguage::Chinese => format!("{count} 个文件"),
        AppLanguage::English => format!("{count} files"),
    }
}

pub fn relative_time(language: AppLanguage, timestamp: DateTime<Local>) -> String {
    let seconds = Local::now()
        .signed_duration_since(timestamp)
        .num_seconds()
        .max(0);

    if seconds < 60 {
        return match language {
            AppLanguage::Chinese => "刚刚".to_string(),
            AppLanguage::English => "Just now".to_string(),
        };
    }

    let minutes = seconds / 60;
    if minutes < 60 {
        return match language {
            AppLanguage::Chinese => format!("{minutes}分钟前"),
            AppLanguage::English => format!(
                "{minutes} minute{} ago",
                if minutes == 1 { "" } else { "s" }
            ),
        };
    }

    let hours = minutes / 60;
    if hours < 24 {
        return match language {
            AppLanguage::Chinese => format!("{hours}小时前"),
            AppLanguage::English => {
                format!("{hours} hour{} ago", if hours == 1 { "" } else { "s" })
            }
        };
    }

    let days = hours / 24;
    if days < 30 {
        return match language {
            AppLanguage::Chinese => format!("{days}天前"),
            AppLanguage::English => format!("{days} day{} ago", if days == 1 { "" } else { "s" }),
        };
    }

    let months = days / 30;
    if months < 12 {
        return match language {
            AppLanguage::Chinese => format!("{months}个月前"),
            AppLanguage::English => {
                format!("{months} month{} ago", if months == 1 { "" } else { "s" })
            }
        };
    }

    let years = months / 12;
    match language {
        AppLanguage::Chinese => format!("{years}年前"),
        AppLanguage::English => format!("{years} year{} ago", if years == 1 { "" } else { "s" }),
    }
}
