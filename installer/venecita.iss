; Windows installer for Venecita Dock (Inno Setup 6). Build it with:
;
;   iscc /DVersion=0.1.3 /DBinDir=..\target\release /O..\dist /Fvenecita-dock-setup installer\venecita.iss
;
; (the release workflow does it). It installs for the current user only, without
; administrator rights, into %LOCALAPPDATA%\Programs\Venecita Dock: copies the programs and
; their documents, adds a Start menu shortcut (and a desktop one if chosen), puts the folder
; on the user's PATH so `venecita` works in a terminal, and starts Venecita Dock at the end.
; Running a newer setup over an older one updates it (same AppId; never change it). The
; MSI used up to 0.1.3 is removed first if it is installed.

#ifndef Version
  #define Version "0.0.0"
#endif
#ifndef BinDir
  #define BinDir "..\target\release"
#endif

[Setup]
AppId={{6E0B7C1A-3F4D-4C55-9B7E-2D8A4F1C9E21}
AppName=Venecita Dock
AppVersion={#Version}
AppVerName=Venecita Dock {#Version}
AppPublisher=Venecita Dock contributors
AppPublisherURL=https://github.com/pimgux/venecita
AppSupportURL=https://github.com/pimgux/venecita/issues
DefaultDirName={autopf}\Venecita Dock
DefaultGroupName=Venecita Dock
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
ChangesEnvironment=yes
CloseApplications=force
RestartApplications=no
SetupIconFile=..\crates\venecita-app\assets\icons\AppIcon.ico
UninstallDisplayIcon={app}\venecita-dock.exe
UninstallDisplayName=Venecita Dock
LicenseFile=..\LICENSE
WizardStyle=modern
Compression=lzma2
SolidCompression=yes
OutputDir=..\dist
OutputBaseFilename=venecita-dock-setup

[Languages]
Name: "en"; MessagesFile: "compiler:Default.isl"
Name: "es"; MessagesFile: "compiler:Languages\Spanish.isl"

[CustomMessages]
en.LaunchApp=Start Venecita Dock
es.LaunchApp=Abrir Venecita Dock
en.DesktopIcon=Create a desktop shortcut
es.DesktopIcon=Crear un acceso directo en el escritorio
en.Removing=Removing the previous version (MSI)…
es.Removing=Quitando la versión anterior (MSI)…

[Tasks]
Name: "desktopicon"; Description: "{cm:DesktopIcon}"; Flags: unchecked

[Files]
Source: "{#BinDir}\venecita-dock.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#BinDir}\venecita.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\examples\config.toml"; DestDir: "{app}"; DestName: "config.example.toml"; Flags: ignoreversion
Source: "..\docs\API.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\README.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\LICENSE"; DestDir: "{app}"; DestName: "LICENSE.txt"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\Venecita Dock"; Filename: "{app}\venecita-dock.exe"; WorkingDir: "{app}"
Name: "{autodesktop}\Venecita Dock"; Filename: "{app}\venecita-dock.exe"; WorkingDir: "{app}"; Tasks: desktopicon

[Run]
; Start the installed app. A postinstall entry without `skipifsilent` also runs in a silent
; update (/SILENT, as the in-app updater does), so the app comes back after updating.
Filename: "{app}\venecita-dock.exe"; Description: "{cm:LaunchApp}"; Flags: nowait postinstall

[UninstallRun]
; Quit the running app so its files can be removed.
Filename: "{sys}\taskkill.exe"; Parameters: "/f /im venecita-dock.exe"; Flags: runhidden; RunOnceId: "QuitApp"

[Code]
const
  EnvKey = 'Environment';
  // UpgradeCode of the MSI used up to 0.1.3 (wix/venecita.wxs).
  OldMsiUpgradeCode = '{930C4566-00A7-4418-86B1-0E5CE90F0F37}';

// Whether `dir` is already one of the entries of the user's PATH.
function PathHas(Path, Dir: string): Boolean;
begin
  Result := Pos(';' + Uppercase(Dir) + ';', ';' + Uppercase(Path) + ';') > 0;
end;

procedure AddToPath(Dir: string);
var
  Path: string;
begin
  if not RegQueryStringValue(HKCU, EnvKey, 'Path', Path) then
    Path := '';
  if PathHas(Path, Dir) then
    exit;
  if (Path <> '') and (Copy(Path, Length(Path), 1) <> ';') then
    Path := Path + ';';
  RegWriteExpandStringValue(HKCU, EnvKey, 'Path', Path + Dir);
end;

procedure RemoveFromPath(Dir: string);
var
  Path: string;
  P: Integer;
begin
  if not RegQueryStringValue(HKCU, EnvKey, 'Path', Path) then
    exit;
  Path := ';' + Path + ';';
  P := Pos(';' + Uppercase(Dir) + ';', Uppercase(Path));
  if P = 0 then
    exit;
  Delete(Path, P, Length(Dir) + 1);
  // Drop the leading and trailing separators added above.
  Path := Copy(Path, 2, Length(Path) - 2);
  RegWriteExpandStringValue(HKCU, EnvKey, 'Path', Path);
end;

// The old MSI and this installer use the same folder: uninstall the MSI quietly first, so
// there is one Venecita Dock in "Installed apps".
procedure RemoveOldMsi;
var
  Installer, Products: Variant;
  I, Code: Integer;
begin
  try
    Installer := CreateOleObject('WindowsInstaller.Installer');
    Products := Installer.RelatedProducts(OldMsiUpgradeCode);
    for I := 0 to Products.Count - 1 do
    begin
      WizardForm.StatusLabel.Caption := CustomMessage('Removing');
      Exec(ExpandConstant('{sys}\msiexec.exe'), '/x ' + Products.Item(I) + ' /qn /norestart',
        '', SW_HIDE, ewWaitUntilTerminated, Code);
      Log(Format('Removed MSI %s (exit code %d)', [Products.Item(I), Code]));
    end;
  except
    Log('Could not look for the old MSI: ' + GetExceptionMessage);
  end;
end;

procedure CurStepChanged(CurStep: TSetupStep);
begin
  if CurStep = ssInstall then
    RemoveOldMsi;
  if CurStep = ssPostInstall then
    AddToPath(ExpandConstant('{app}'));
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
begin
  if CurUninstallStep = usPostUninstall then
    RemoveFromPath(ExpandConstant('{app}'));
end;
