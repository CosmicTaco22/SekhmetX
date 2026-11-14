//! Bilingual localization (English / Español) for SekhmetX.

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Lang {
    #[default]
    En,
    Es,
}

impl Lang {
    pub fn code(self) -> &'static str {
        match self {
            Lang::En => "en",
            Lang::Es => "es",
        }
    }

    pub fn from_code(s: &str) -> Self {
        if s.eq_ignore_ascii_case("es") || s.to_ascii_lowercase().starts_with("es") {
            Lang::Es
        } else {
            Lang::En
        }
    }

    pub fn toggle(self) -> Self {
        match self {
            Lang::En => Lang::Es,
            Lang::Es => Lang::En,
        }
    }
}

pub struct Strings {
    pub open: &'static str,
    pub open_hover: &'static str,
    pub browser_hover: &'static str,
    pub wl: &'static str,
    pub scroll: &'static str,
    pub pan: &'static str,
    pub zoom: &'static str,
    pub rot: &'static str,
    pub orbit_3d: &'static str,
    pub roi: &'static str,
    pub clear_rois: &'static str,
    pub ortho_mpr: &'static str,
    pub mpr_3d: &'static str,
    pub view: &'static str,
    pub search_hint: &'static str,
    pub all: &'static str,
    pub no_studies: &'static str,
    pub drop_hint: &'static str,
    pub library: &'static str,
    pub studies: &'static str,
    pub clear: &'static str,
    pub remove_from_library: &'static str,
    pub compare: &'static str,
    pub images: &'static str,
    pub force_3d_title: &'static str,
    pub force_3d_headline: &'static str,
    pub force_3d_body: &'static str,
    pub force_3d_yes: &'static str,
    pub force_3d_no: &'static str,
    pub updates_title: &'static str,
    pub current_version: &'static str,
    pub checking_updates: &'static str,
    pub up_to_date: &'static str,
    pub check_again: &'static str,
    pub new_version: &'static str,
    pub download_install: &'static str,
    pub open_github: &'static str,
    pub installing_update: &'static str,
    pub restart_sekhmetx: &'static str,
    pub retry: &'static str,
    pub reset_viewport: &'static str,
    pub flip_h: &'static str,
    pub flip_v: &'static str,
    pub invert: &'static str,
    pub ref_lines: &'static str,
    pub overlays: &'static str,
    pub sync_label: &'static str,
}

pub const EN: Strings = Strings {
    open: "Open",
    open_hover: "Open folder, DICOM files, or .zip archive (Ctrl+O)",
    browser_hover: "Show / Hide Study Browser",
    wl: "WL",
    scroll: "Scroll",
    pan: "Pan",
    zoom: "Zoom",
    rot: "Rot",
    orbit_3d: "3D",
    roi: "ROI",
    clear_rois: "Clear ROIs",
    ortho_mpr: "Ax·Cor·Sag",
    mpr_3d: "3D",
    view: "View",
    search_hint: "Search patient, ID, study, series…",
    all: "All",
    no_studies: "No studies loaded.",
    drop_hint: "Use “Open” or drop a folder / .zip onto the window.",
    library: "Library",
    studies: "studies",
    clear: "Clear",
    remove_from_library: "Remove from Library",
    compare: "Compare:",
    images: "images",
    force_3d_title: "Low-Quality 3D Reconstruction Warning",
    force_3d_headline: "This acquisition does not have enough slices or 3D orientation data for standard multiplanar reconstruction.",
    force_3d_body: "Forcing 3D slicing on this series may produce a blurry, stretched, or low-quality image.\n\nAre you sure you want to continue?",
    force_3d_yes: "Yes, Force 3D",
    force_3d_no: "No, Cancel",
    updates_title: "SekhmetX Updates",
    current_version: "Current version",
    checking_updates: "Checking GitHub Releases for updates…",
    up_to_date: "You are running the latest version",
    check_again: "Check Again",
    new_version: "New version available",
    download_install: "Download & Install",
    open_github: "Open GitHub Release",
    installing_update: "Downloading and installing update…",
    restart_sekhmetx: "Restart SekhmetX",
    retry: "Retry",
    reset_viewport: "Reset Viewport (R)",
    flip_h: "Flip Horizontal (H)",
    flip_v: "Flip Vertical (V)",
    invert: "Invert (I)",
    ref_lines: "Reference Lines",
    overlays: "Annotations / HUD",
    sync_label: "Sync:",
};

pub const ES: Strings = Strings {
    open: "Abrir",
    open_hover: "Abrir carpeta, archivos DICOM o archivo .zip (Ctrl+O)",
    browser_hover: "Mostrar / Ocultar Explorador de Estudios",
    wl: "NV",
    scroll: "Cortes",
    pan: "Mover",
    zoom: "Zoom",
    rot: "Rotar",
    orbit_3d: "3D",
    roi: "ROI",
    clear_rois: "Borrar ROIs",
    ortho_mpr: "Ax·Cor·Sag",
    mpr_3d: "3D",
    view: "Vista",
    search_hint: "Buscar paciente, ID, estudio, serie…",
    all: "Todos",
    no_studies: "No hay estudios cargados.",
    drop_hint: "Use “Abrir” o arrastre una carpeta / .zip a la ventana.",
    library: "Biblioteca",
    studies: "estudios",
    clear: "Limpiar",
    remove_from_library: "Quitar de la Biblioteca",
    compare: "Comparar:",
    images: "imágenes",
    force_3d_title: "Advertencia de Reconstrucción 3D de Baja Calidad",
    force_3d_headline: "Esta adquisición no tiene suficientes cortes u orientación 3D para una reconstrucción multiplanar estándar.",
    force_3d_body: "Forzar el corte 3D en esta serie puede generar una imagen borrosa, estirada o de baja calidad.\n\n¿Está seguro de que desea continuar?",
    force_3d_yes: "Sí, Forzar 3D",
    force_3d_no: "No, Cancelar",
    updates_title: "Actualizaciones de SekhmetX",
    current_version: "Versión actual",
    checking_updates: "Buscando actualizaciones en GitHub…",
    up_to_date: "Está usando la versión más reciente",
    check_again: "Buscar de nuevo",
    new_version: "Nueva versión disponible",
    download_install: "Descargar e Instalar",
    open_github: "Abrir en GitHub",
    installing_update: "Descargando e instalando actualización…",
    restart_sekhmetx: "Reiniciar SekhmetX",
    retry: "Reintentar",
    reset_viewport: "Restablecer Visor (R)",
    flip_h: "Voltear Horizontal (H)",
    flip_v: "Voltear Vertical (V)",
    invert: "Invertir (I)",
    ref_lines: "Líneas de Referencia",
    overlays: "Anotaciones / HUD",
    sync_label: "Sincr.:",
};

pub fn tr(lang: Lang) -> &'static Strings {
    match lang {
        Lang::En => &EN,
        Lang::Es => &ES,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn language_roundtrip_and_fallback() {
        assert_eq!(Lang::from_code("es"), Lang::Es);
        assert_eq!(Lang::from_code("es-AR"), Lang::Es);
        assert_eq!(Lang::from_code("en"), Lang::En);
        assert_eq!(Lang::from_code("unknown"), Lang::En);
        assert_eq!(Lang::Es.code(), "es");
        assert_eq!(Lang::En.toggle(), Lang::Es);
    }
}
