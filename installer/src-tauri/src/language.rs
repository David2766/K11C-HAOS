pub fn image_picker(locale: Option<&str>) -> (&'static str, &'static str) {
    match locale {
        Some("ko") => ("HAOS 이미지 선택", "HAOS 이미지"),
        Some("zh-TW") => ("選擇 HAOS 映像", "HAOS 映像"),
        Some("es") => ("Seleccionar imagen de HAOS", "Imagen de HAOS"),
        Some("ja") => ("HAOS イメージを選択", "HAOS イメージ"),
        _ => ("Select HAOS image", "HAOS image"),
    }
}
pub fn factory_picker(locale:Option<&str>)->(&'static str,&'static str){
    match locale.unwrap_or("en"){
        "ko"=>("제조사 이미지 선택","전체 시스템 이미지"),
        "zh-TW"=>("選擇原廠映像","完整系統映像"),
        "es"=>("Seleccionar imagen del fabricante","Imagen del sistema completo"),
        "ja"=>("メーカーのイメージを選択","システム全体のイメージ"),
        _=>("Select manufacturer image","Complete system image")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fixed_labels_for_all_supported_languages() {
        for (locale, title, filter) in [
            ("en", "Select HAOS image", "HAOS image"),
            ("ko", "HAOS 이미지 선택", "HAOS 이미지"),
            ("zh-TW", "選擇 HAOS 映像", "HAOS 映像"),
            ("es", "Seleccionar imagen de HAOS", "Imagen de HAOS"),
            ("ja", "HAOS イメージを選択", "HAOS イメージ"),
        ] {
            assert_eq!(image_picker(Some(locale)), (title, filter));
        }
    }
    #[test]
    fn unknown_or_missing_locale_never_becomes_dialog_text() {
        for locale in [None, Some(""), Some("de"), Some("zh-CN"), Some("C:/arbitrary.img")] {
            assert_eq!(image_picker(locale), ("Select HAOS image", "HAOS image"));
            assert_eq!(backup_picker(locale), ("Select a backup to restore", "K11C backup or full disk image"));
        }
    }
    #[test]
    fn backup_labels_for_all_supported_languages() {
        for (locale,title) in [
            ("en","Select a backup to restore"),("ko","복원할 백업 선택"),
            ("zh-TW","選擇要還原的備份"),("es","Seleccionar copia para restaurar"),
            ("ja","復元するバックアップを選択")
        ] { assert_eq!(backup_picker(Some(locale)).0,title); }
    }
    #[test] fn manufacturer_picker_uses_fixed_localized_labels(){
        for (locale,title) in [("en","Select manufacturer image"),("ko","제조사 이미지 선택"),("zh-TW","選擇原廠映像"),("es","Seleccionar imagen del fabricante"),("ja","メーカーのイメージを選択")]{assert_eq!(factory_picker(Some(locale)).0,title);}
        for locale in [None,Some("zh-CN"),Some("C:/arbitrary")]{assert_eq!(factory_picker(locale),("Select manufacturer image","Complete system image"));}
    }
}
pub fn backup_picker(locale:Option<&str>)->(&'static str,&'static str){
    match locale.unwrap_or("en"){
        "ko"=>("복원할 백업 선택","K11C 백업 또는 전체 디스크 이미지"),
        "zh-TW"=>("選擇要還原的備份","K11C 備份或完整磁碟映像"),
        "es"=>("Seleccionar copia para restaurar","Copia K11C o imagen de disco completo"),
        "ja"=>("復元するバックアップを選択","K11C バックアップまたはディスク全体のイメージ"),
        _=>("Select a backup to restore","K11C backup or full disk image")
    }
}
