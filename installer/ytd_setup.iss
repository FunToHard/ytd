; Inno Setup Script for YTD
; Compiles into a single standalone installer: YTD-Setup.exe

#define MyAppName "YTD"
#ifndef MyAppVersion
; x-release-please-start-version
#define MyAppVersion "1.0.7"
; x-release-please-end
#endif
#define MyAppPublisher "YTD Project"
#define MyAppURL "https://github.com/FunToHard/ytd"
#define MyAppExeName "ytd-daemon.exe"

[Setup]
AppId={{D37F2C74-8A89-4D2A-9DF2-87B14002C931}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppPublisher={#MyAppPublisher}
AppPublisherURL={#MyAppURL}
AppSupportURL={#MyAppURL}
AppUpdatesURL={#MyAppURL}
DefaultDirName={autopf}\{#MyAppName}
DisableProgramGroupPage=yes
OutputBaseFilename=YTD-Setup
Compression=lzma2/ultra64
SolidCompression=yes
WizardStyle=modern
PrivilegesRequired=lowest
OutputDir=..\target\installer
SetupIconFile=..\daemon\resources\app-icon.ico
LicenseFile=..\LICENSE
UninstallDisplayIcon={app}\{#MyAppExeName}
CloseApplications=force
RestartApplications=no

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked
Name: "startmenuicon"; Description: "Create a Start Menu shortcut"; GroupDescription: "{cm:AdditionalIcons}"
Name: "autostart"; Description: "Start YTD automatically when Windows starts"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Registry]
; Auto-start task integration
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: string; ValueName: "YTD"; ValueData: """{app}\{#MyAppExeName}"""; Flags: uninsdeletevalue; Tasks: autostart
; Clean up YTD auto-start entry on uninstall even if enabled via tray menu
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueName: "YTD"; Flags: dontcreatekey uninsdeletevalue
; Clean up legacy corrupted keys created by prior builds
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"""; Flags: dontcreatekey uninsdeletekey
Root: HKCU; Subkey: "Software\Classes\AppUserModelId\YTD"""; Flags: dontcreatekey uninsdeletekey

[Files]
; Main Executable
Source: "..\target\release\{#MyAppExeName}"; DestDir: "{app}"; Flags: ignoreversion
; Browser Extension
Source: "..\extension\*"; DestDir: "{app}\extension"; Flags: ignoreversion recursesubdirs createallsubdirs
; Documentation & License
Source: "..\README.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\LICENSE"; DestDir: "{app}"; Flags: ignoreversion
; Icon
Source: "..\daemon\resources\app-icon.ico"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"; IconFilename: "{app}\app-icon.ico"
Name: "{autodesktop}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"; IconFilename: "{app}\app-icon.ico"; Tasks: desktopicon

[Run]
Filename: "{app}\{#MyAppExeName}"; Description: "{cm:LaunchProgram,{#StringChange(MyAppName, '&', '&&')}}"; Flags: nowait postinstall

[Code]
// Terminate running ytd-daemon.exe instances before updating files
procedure KillRunningApp();
var
  ResultCode: Integer;
begin
  Exec('taskkill.exe', '/F /IM ytd-daemon.exe', '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
  Sleep(300);
end;

function InitializeSetup(): Boolean;
begin
  KillRunningApp();
  Result := True;
end;

function PrepareToInstall(var NeedsRestart: Boolean): String;
begin
  KillRunningApp();
  Result := '';
end;
