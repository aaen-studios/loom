; loom's Windows installer, built by .github/workflows/release.yml with Inno Setup:
;
;     iscc /DVersion=0.3.0 /DExe=path\to\loom.exe desktop\installer.iss
;
; It installs for the person alone, in %LOCALAPPDATA%\Programs\loom, so loom can
; update itself there without asking for an administrator (src/shell.rs,
; apply_update). The Start menu shortcut carries loom's AppUserModelID, which
; Windows shows notifications under.

#ifndef Version
  #define Version "0.0.0"
#endif
#ifndef Exe
  #define Exe "target\release\loom.exe"
#endif

[Setup]
AppId={{6F7B4E2A-3C1D-4B8E-9A5F-2D7C8E1B4A90}
AppName=loom
AppVersion={#Version}
AppPublisher=Aaen Studios
AppPublisherURL=https://github.com/aaen-studios/loom
DefaultDirName={localappdata}\Programs\loom
DisableDirPage=yes
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
OutputBaseFilename=loom-setup
OutputDir=.
SetupIconFile=..\design\logo\app.ico
UninstallDisplayIcon={app}\loom.exe
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
CloseApplications=yes
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible

[Files]
Source: "{#Exe}"; DestDir: "{app}"; DestName: "loom.exe"; Flags: ignoreversion

[Icons]
Name: "{userprograms}\loom"; Filename: "{app}\loom.exe"; AppUserModelID: "AaenStudios.Loom"

[Run]
Filename: "{app}\loom.exe"; Description: "Open loom"; Flags: nowait postinstall skipifsilent

[UninstallDelete]
Type: files; Name: "{app}\loom.old.exe"
