; Windows installer for Visual Novel Studio (Inno Setup 6).
; Built by .github/workflows/release.yml:
;   ISCC /DAppVersion=0.1.0 /DDist=..\dist /DOutName=vn-studio-v0.1.0-windows-setup installer\vn-studio.iss
;
; The install is per-user (no administrator rights): the editor's own
; "update" button rewrites files in this folder, so it must stay writable.
; Keep this file ASCII-only: Inno Setup reads files without a BOM as ANSI.

#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif
#ifndef Dist
  #define Dist "..\dist"
#endif
#ifndef OutName
  #define OutName "vn-studio-windows-setup"
#endif

[Setup]
; AppId must never change: it is how a newer installer finds the old copy.
AppId={{6B0E4C1F-2B7A-4D53-9C55-7A1F4E0D9A31}
AppName=Visual Novel Studio
AppVersion={#AppVersion}
AppPublisher=kclazzy
AppPublisherURL=https://github.com/kclazzy/visualNovel
AppSupportURL=https://github.com/kclazzy/visualNovel/issues
AppUpdatesURL=https://github.com/kclazzy/visualNovel/releases
PrivilegesRequired=lowest
DefaultDirName={autopf}\Visual Novel Studio
DisableProgramGroupPage=yes
UninstallDisplayIcon={app}\vn-studio.exe
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
CloseApplications=yes
OutputDir=..
OutputBaseFilename={#OutName}

[Languages]
Name: "russian"; MessagesFile: "compiler:Languages\Russian.isl"
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"

[Files]
Source: "{#Dist}\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs

[Icons]
Name: "{autoprograms}\Visual Novel Studio"; Filename: "{app}\vn-studio.exe"
Name: "{autodesktop}\Visual Novel Studio"; Filename: "{app}\vn-studio.exe"; Tasks: desktopicon

[Run]
Filename: "{app}\vn-studio.exe"; Description: "{cm:LaunchProgram,Visual Novel Studio}"; Flags: nowait postinstall skipifsilent

[UninstallDelete]
; Previous copies left by the in-app updater.
Type: files; Name: "{app}\*.old"
