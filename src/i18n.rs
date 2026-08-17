use crate::model::AppLanguage;

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
        AppLanguage::Chinese => format!("{count} 个字符"),
        AppLanguage::English => format!("{count} characters"),
    }
}

pub fn file_count(language: AppLanguage, count: usize) -> String {
    match language {
        AppLanguage::Chinese => format!("{count} 个文件"),
        AppLanguage::English => format!("{count} files"),
    }
}
