; gif-ide Inno Setup script
; CIからのビルド: ISCC.exe /DMyAppVersion=x.y.z installer\gif-ide.iss
; ローカル単体実行時はダミーバージョン (0.0.0) にフォールバックする

#ifndef MyAppVersion
  #define MyAppVersion "0.0.0"
#endif

#define MyAppName "gif-ide"
#define MyAppPublisher "Flupinochan"
#define MyAppURL "https://github.com/Flupinochan/gif-ide"
#define MyAppExeName "gif-ide.exe"

[Setup]
; 固定GUID。バージョンが変わっても絶対に変更しないこと (Windowsのアップグレード判定に使われる)
AppId={{d4e06ee3-fbc5-4ffa-bcaa-71efb70b70e4}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppPublisher={#MyAppPublisher}
AppPublisherURL={#MyAppURL}
AppSupportURL={#MyAppURL}
AppUpdatesURL={#MyAppURL}
DefaultDirName={autopf}\{#MyAppName}
DefaultGroupName={#MyAppName}
DisableProgramGroupPage=yes
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
SetupIconFile=..\ui\ico\app.ico
UninstallDisplayIcon={app}\{#MyAppExeName}
LicenseFile=..\LICENSE
Compression=lzma2/ultra64
SolidCompression=yes
OutputDir=..\dist
OutputBaseFilename=gif-ide-v{#MyAppVersion}-win64-setup
WizardStyle=modern

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "Create a &desktop icon"; GroupDescription: "Additional icons:"; Flags: unchecked

[Files]
; CIが組み立てるdist/gif-ide/ (gif-ide.exe + ffmpeg\配下3ファイル) をそのまま再帰コピー。
; 将来ffmpeg以外のファイルが増えてもこの1行で追従できる
Source: "..\dist\gif-ide\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{group}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"
Name: "{group}\Uninstall {#MyAppName}"; Filename: "{uninstallexe}"
Name: "{autodesktop}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"; Tasks: desktopicon

[Run]
Filename: "{app}\{#MyAppExeName}"; Description: "Launch {#MyAppName}"; Flags: nowait postinstall skipifsilent
