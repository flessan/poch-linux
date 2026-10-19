; Inno Setup script for Poch  (https://jrsoftware.org/isinfo.php)
; Build: run build-installer.bat (builds the release exe, then compiles this script)

#define MyAppName "Poch"
#define MyAppVersion "0.3.0"
#define MyAppExe "poch.exe"

[Setup]
AppId={{922ADA92-839C-4277-A579-4346348064A6}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppPublisher=Commonly Works
DefaultDirName={autopf}\{#MyAppName}
DisableProgramGroupPage=yes
DisableDirPage=auto
; per-user install: no admin prompt, and the startup entry (HKCU) belongs to the same user
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
SetupIconFile=icon.ico
UninstallDisplayIcon={app}\{#MyAppExe}
UninstallDisplayName={#MyAppName}
OutputDir=installer_output
OutputBaseFilename=Poch-Setup-{#MyAppVersion}
Compression=lzma2
SolidCompression=yes
WizardStyle=modern

[Tasks]
Name: "startup"; Description: "Start Poch automatically when Windows starts"; GroupDescription: "Options:"
Name: "desktopicon"; Description: "Create a &desktop shortcut"; GroupDescription: "Options:"; Flags: unchecked

[Files]
Source: "target\release\{#MyAppExe}"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\{#MyAppName}"; Filename: "{app}\{#MyAppExe}"
Name: "{autodesktop}\{#MyAppName}"; Filename: "{app}\{#MyAppExe}"; Tasks: desktopicon

[Registry]
; same value the app's own "Start with Windows" menu item reads and writes
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: string; ValueName: "Poch"; ValueData: """{app}\{#MyAppExe}"""; Tasks: startup; Flags: uninsdeletevalue

[Run]
Filename: "{app}\{#MyAppExe}"; Description: "Launch Poch"; Flags: nowait postinstall skipifsilent

[Code]
procedure StopPoch();
var
  R: Integer;
begin
  Exec(ExpandConstant('{sys}\taskkill.exe'), '/F /IM {#MyAppExe}', '', SW_HIDE, ewWaitUntilTerminated, R);
end;

// close a running Poch before files are replaced (upgrade)
function PrepareToInstall(var NeedsRestart: Boolean): String;
begin
  StopPoch();
  Result := '';
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
begin
  if CurUninstallStep = usUninstall then
  begin
    StopPoch();
    // remove the startup entry even if it was enabled later from inside the app
    RegDeleteValue(HKEY_CURRENT_USER, 'Software\Microsoft\Windows\CurrentVersion\Run', 'Poch');
  end;
end;
