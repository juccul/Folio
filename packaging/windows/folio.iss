#ifndef PayloadDir
  #error Define PayloadDir as the portable package directory.
#endif
#ifndef FolioVersion
  #define FolioVersion "0.1.3"
#endif
[Setup]
AppId={{A8A523AB-784B-4AC1-9D4C-B0D9B378AA62}
AppName=Folio
AppVersion={#FolioVersion}
DefaultDirName={localappdata}\Programs\Folio
DefaultGroupName=Folio
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
OutputBaseFilename=folio-{#FolioVersion}-windows-x64-setup
Compression=lzma2
SolidCompression=yes
UninstallDisplayIcon={app}\bin\folio.exe
LicenseFile={#PayloadDir}\LICENSE
CloseApplications=yes
RestartApplications=no
[Files]
Source: "{#PayloadDir}\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs
[Icons]
Name: "{group}\Folio"; Filename: "{app}\bin\folio.exe"
[Run]
Filename: "{app}\bin\folio.exe"; Description: "Open Folio"; Flags: nowait postinstall skipifsilent
