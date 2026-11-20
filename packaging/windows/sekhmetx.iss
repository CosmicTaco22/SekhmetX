; SekhmetX Windows installer (Inno Setup 6). Built by .github/workflows/release.yml:
;   iscc /DAppVersion=0.7.0 /DArch=x64compatible /DRepoDir=<checkout> /DSourceDir=<dist\sekhmetx>
;        /DOutputDir=<dist> /DOutputName=sekhmetx-v0.7.0-windows-x86_64-setup packaging\windows\sekhmetx.iss
; Installs for the current user only (no administrator rights), into
; %LOCALAPPDATA%\Programs\SekhmetX, so the in-app updater can replace the program.
; The library and settings live in %LOCALAPPDATA%\SekhmetX and are not touched.

#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif
#ifndef Arch
  #define Arch "x64compatible"
#endif
#ifndef RepoDir
  #define RepoDir "..\.."
#endif
#ifndef SourceDir
  #define SourceDir RepoDir + "\dist\sekhmetx"
#endif
#ifndef OutputDir
  #define OutputDir RepoDir + "\dist"
#endif
#ifndef OutputName
  #define OutputName "sekhmetx-setup"
#endif

[Setup]
AppId={{8F3C2B1E-5D7A-4C9E-9B21-6A0E4F7D2C13}
AppName=SekhmetX
AppVersion={#AppVersion}
AppVerName=SekhmetX {#AppVersion}
AppPublisher=SekhmetX contributors
AppPublisherURL=https://github.com/CosmicTaco22/SekhmetX
AppSupportURL=https://github.com/CosmicTaco22/SekhmetX/issues
AppUpdatesURL=https://github.com/CosmicTaco22/SekhmetX/releases
VersionInfoVersion={#AppVersion}
DefaultDirName={localappdata}\Programs\SekhmetX
DefaultGroupName=SekhmetX
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
ArchitecturesAllowed={#Arch}
ArchitecturesInstallIn64BitMode={#Arch}
OutputDir={#OutputDir}
OutputBaseFilename={#OutputName}
SetupIconFile={#RepoDir}\assets\icon\sekhmetx.ico
UninstallDisplayIcon={app}\sekhmetx.exe
UninstallDisplayName=SekhmetX
LicenseFile={#RepoDir}\LICENSE
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
CloseApplications=yes

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"
Name: "spanish"; MessagesFile: "compiler:Languages\Spanish.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "{#SourceDir}\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{autoprograms}\SekhmetX"; Filename: "{app}\sekhmetx.exe"
Name: "{autodesktop}\SekhmetX"; Filename: "{app}\sekhmetx.exe"; Tasks: desktopicon

[Run]
Filename: "{app}\sekhmetx.exe"; Description: "{cm:LaunchProgram,SekhmetX}"; Flags: nowait postinstall skipifsilent
