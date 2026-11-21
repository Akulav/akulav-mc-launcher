; Akulav Launcher - Inno Setup script.
; One file installs the launcher the first time and updates it later:
;   * first run: asks for the install folder (the launcher AND its modpacks, worlds and Java go there)
;   * later runs and in-app updates (/UPDATE): reuse the remembered folder without asking
; Build: iscc /DAppVersion=X.Y.Z /DSourceExe=path\to\AkulavLauncher.exe installer\akulav-launcher.iss

#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif
#ifndef SourceExe
  #define SourceExe "..\target\release\AkulavLauncher.exe"
#endif

#define AppName "Akulav Launcher"
#define AppExe "AkulavLauncher.exe"
#define SetupName "AkulavLauncher-setup"
; Marks the install folder; the launcher keeps its data next to it (src/launcher/install).
#define Marker ".akulav-install"

[Setup]
; AppId never changes: installing over an existing copy upgrades it in place, in the same folder.
; The launcher reads the folder back from this key (src/launcher/install/windows.rs).
AppId={{7FBF629D-D100-4E71-AE93-4977A99B250F}
AppName={#AppName}
AppVersion={#AppVersion}
AppVerName={#AppName} {#AppVersion}
AppPublisher=Akulav
AppPublisherURL=https://github.com/Akulav/akulav-mc-launcher
AppSupportURL=https://github.com/Akulav/akulav-mc-launcher/issues
AppUpdatesURL=https://github.com/Akulav/akulav-mc-launcher/releases
AppCopyright=Made by Akulav
VersionInfoVersion={#AppVersion}
VersionInfoCompany=Akulav
VersionInfoDescription={#AppName} Setup
VersionInfoProductName={#AppName}
PrivilegesRequired=lowest
DefaultDirName={localappdata}\Programs\AkulavLauncher
; Ask for the folder on the first install only; updates reuse the previous one.
DisableDirPage=auto
UsePreviousAppDir=yes
DirExistsWarning=no
DisableProgramGroupPage=yes
DefaultGroupName={#AppName}
OutputBaseFilename={#SetupName}
OutputDir=..\dist
SetupIconFile=..\assets\mining.ico
UninstallDisplayIcon={app}\{#AppExe}
UninstallDisplayName={#AppName}
WizardStyle=modern
CloseApplications=force
RestartApplications=no
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
Compression=lzma2/ultra64
SolidCompression=yes
LicenseFile=..\LICENSE

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[CustomMessages]
english.DeleteData=Also delete downloaded modpacks, worlds and settings?%n%nChoose No to keep them for a future reinstall.
english.LaunchApp=Launch Akulav Launcher
english.DataNote=The launcher and everything it downloads (modpacks, worlds, Java) will be stored in this folder.

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"

[Files]
Source: "{#SourceExe}"; DestDir: "{app}"; DestName: "{#AppExe}"; Flags: ignoreversion
Source: "..\LICENSE"; DestDir: "{app}"; DestName: "LICENSE.txt"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\{#AppName}"; Filename: "{app}\{#AppExe}"; WorkingDir: "{app}"; Comment: "Minecraft modpack launcher - made by Akulav"
Name: "{autodesktop}\{#AppName}"; Filename: "{app}\{#AppExe}"; WorkingDir: "{app}"; Comment: "Minecraft modpack launcher - made by Akulav"; Tasks: desktopicon

[Run]
; Interactive install: the usual "Launch" checkbox.
Filename: "{app}\{#AppExe}"; Parameters: "--installed"; WorkingDir: "{app}"; Description: "{cm:LaunchApp}"; Flags: nowait postinstall skipifsilent
; Silent in-app update (/UPDATE /FROM=<old version>): start the new launcher again.
Filename: "{app}\{#AppExe}"; Parameters: "--updated-from ""{param:FROM|}"""; WorkingDir: "{app}"; Flags: nowait; Check: IsUpdate

[UninstallDelete]
Type: files; Name: "{app}\{#Marker}"
; The downloaded update setup (in-app updates keep it in the launcher's own folder).
Type: filesandordirs; Name: "{app}\cache\update"

[Code]
function IsUpdate(): Boolean;
var
  I: Integer;
begin
  Result := False;
  for I := 1 to ParamCount do
    if CompareText(ParamStr(I), '/UPDATE') = 0 then
      Result := True;
end;

procedure InitializeWizard();
begin
  WizardForm.SelectDirLabel.Caption := WizardForm.SelectDirLabel.Caption + #13#10 + CustomMessage('DataNote');
end;

procedure CurStepChanged(CurStep: TSetupStep);
begin
  if CurStep = ssPostInstall then
  begin
    SaveStringToFile(ExpandConstant('{app}\{#Marker}'), '{#AppVersion}', False);
  end;
end;

function HasLauncherData(Dir: String): Boolean;
begin
  Result := FileExists(Dir + '\settings.json') or FileExists(Dir + '\installed.json')
    or DirExists(Dir + '\instances') or DirExists(Dir + '\shared');
end;

{ Only what the launcher itself creates, never anything else the user keeps in that folder. }
procedure DeleteLauncherData(Dir: String);
begin
  DelTree(Dir + '\shared', True, True, True);
  DelTree(Dir + '\instances', True, True, True);
  DelTree(Dir + '\cache', True, True, True);
  DelTree(Dir + '\logs', True, True, True);
  DeleteFile(Dir + '\modpacks_cache.json');
  DeleteFile(Dir + '\window.ron');
  DeleteFile(Dir + '\installed.json');
  DeleteFile(Dir + '\settings.json');
  RemoveDir(Dir);
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
var
  AppDir: String;
begin
  if CurUninstallStep = usPostUninstall then
  begin
    AppDir := ExpandConstant('{app}');
    { Never delete data when silent. }
    if (not UninstallSilent) and HasLauncherData(AppDir) then
      if MsgBox(CustomMessage('DeleteData'), mbConfirmation, MB_YESNO or MB_DEFBUTTON2) = IDYES then
        DeleteLauncherData(AppDir);
    RemoveDir(AppDir);
  end;
end;
