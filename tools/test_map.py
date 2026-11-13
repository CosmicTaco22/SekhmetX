#!/usr/bin/env python3
"""Checks that well-known IsiX/Horos files land in the right area of porting/map.toml.

Run after editing the map:  python3 tools/test_map.py
"""
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import upstream  # noqa: E402

EXPECTED = {
    # 2D viewer, and things that look like it but belong elsewhere
    "Horos/Sources/DCMView.m": "viewer-2d",
    "Horos/Sources/DCMView+Drawing.swift": "viewer-2d",
    "Horos/Sources/ViewerController.m": "viewer-2d",
    "Horos/Sources/LoupeView.m": "viewer-2d",
    "Horos/Sources/ViewerROIVolume.swift": "roi-measurements",
    "Horos/Sources/ViewerFusion.swift": "fusion-pet",
    "Horos/Sources/ViewerMovieExport.swift": "anonymize-export",
    "Horos/Sources/ViewerKeyObjectDocument.swift": "annotations-sr",
    "Horos/Sources/ViewerRegistrationSession.swift": "segmentation-registration",
    "Horos/Sources/ViewerVolumeSession.swift": "volume-3d",
    "Horos/Sources/ViewerPrintJob.swift": "print-burn-mail",
    # "SR" means surface rendering in OsiriX/Horos, except SRAnnotation (structured report)
    "Horos/Sources/SRController.m": "volume-3d",
    "Horos/Sources/SRAnnotation.m": "annotations-sr",
    # reconstruction
    "Horos/Sources/MPRMetalReslicer.swift": "mpr",
    "Horos/Sources/OrthogonalMPRViewer.m": "mpr",
    "Horos/Sources/OrthogonalMPRPETCTViewer.m": "fusion-pet",
    "Horos/Sources/CPRController.mm": "cpr",
    "Horos/Sources/CurvedMPR.m": "cpr",
    # data, files, network
    "Horos/Sources/BrowserController.m": "browser",
    "Horos/Sources/DicomDatabase.m": "database-import",
    "Horos/Sources/ArchiveImport.swift": "database-import",
    "Horos/Sources/ITKVolumeImport.swift": "volume-3d",
    "Horos/Sources/DicomFile.m": "dicom-core",
    "Horos/Sources/DCMPix.m": "imaging",
    "Horos/Sources/ROI.m": "roi-measurements",
    "Horos/Sources/QueryController.m": "network-pacs",
    "Horos/Sources/DICOMwebClient.swift": "dicomweb",
    "Horos/Sources/T2FitMap.swift": "advanced-mr",
    "Horos/Sources/WebPortal.m": "web-sharing",
    "Horos/Sources/PluginManager.m": "plugins",
    "Horos/Sources/AppController.m": "app-shell",
    # IsiX modules (sources and tests)
    "Modules/IsiXImaging/Sources/VOILUTMapping.swift": "imaging",
    "Modules/IsiXImaging/Sources/SUVParameters.swift": "fusion-pet",
    "Modules/IsiXImaging/Sources/BufferOperations/T1Fitting.swift": "advanced-mr",
    "Modules/IsiXImaging/Sources/BufferOperations/WindowRendering.swift": "imaging",
    "Modules/IsiXImaging/Sources/ViewerWindowLevel/WindowLevelDrag.swift": "viewer-2d",
    "Modules/IsiXImaging/Tests/IsiXImagingTests/VOILUTMappingTests.swift": "imaging",
    "Modules/IsiXROI/Tests/IsiXROITests/ROIPixelStatisticsTests.swift": "roi-measurements",
    "Modules/IsiXGeometry/Sources/Plane3.swift": "geometry",
    "Modules/IsiXDICOMNetwork/Sources/DICOMwebSearch.swift": "network-pacs",
    # folders
    "Horos/Resources/CLUTs/Hot Iron.plist": "data-assets",
    "Horos/Resources/Icons/ROI.tiff": "translations-icons",
    "Horos/Resources/fr.lproj/Localizable.strings": "translations-icons",
    "Horos/Sources/ThirdParty/libarchive/archive.h": "third-party",
    "Nitrogen/Sources/N2Button.swift": "ui-toolkit",
    "DCM Framework/DCMObject.m": "dicom-core",
    "Binaries/Splash/images/Icon\r": "meta",
    "README.md": "meta",
}


def main() -> int:
    areas = upstream.load_map()
    bad = 0
    for path, want in EXPECTED.items():
        area = upstream.classify(path, areas)
        got = area.id if area else "(none)"
        if got != want:
            bad += 1
            print(f"FAIL {path!r}: in {got}, expected {want}")
    print(f"{len(EXPECTED) - bad}/{len(EXPECTED)} files in the expected area")
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main())
