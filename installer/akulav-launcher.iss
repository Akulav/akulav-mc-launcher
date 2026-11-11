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

[InstallDelete]
; Leftovers of the self-updating 10.x launcher.
Type: files; Name: "{app}\AkulavLauncher.old.exe"
Type: files; Name: "{app}\AkulavLauncher.new.exe"

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
Type: files; Name: "{app}\AkulavLauncher.old.exe"
Type: files; Name: "{app}\AkulavLauncher.new.exe"
; The downloaded update setup (in-app updates keep it in the launcher's own folder).
Type: filesandordirs; Name: "{app}\cache\update"
; Window size and position remembered by the UI toolkit before 11.0.1 (now window.ron in {app}).
Type: filesandordirs; Name: "{userappdata}\akulav-launcher"

[Code]
const
  LegacyUninstallKey = 'Software\Microsoft\Windows\CurrentVersion\Uninstall\AkulavLauncher';

function IsUpdate(): Boolean;
var
  I: Integer;
begin
  Result := False;
  for I := 1 to ParamCount do
    if CompareText(ParamStr(I), '/UPDATE') = 0 then
      Result := True;
end;

{ A 10.x launcher updates by renaming the downloaded file to AkulavLauncher.exe inside its own
  folder and starting it. Installing there would have to overwrite this very file, and Setup
  can't copy its own file while it runs, so let cmd wait until we've exited, copy it to the temp
  folder and start it from there. The restarted setup shows the normal wizard. }
function InitializeSetup(): Boolean;
var
  Src, TempSetup: String;
  Code: Integer;
begin
  Result := True;
  Src := ExpandConstant('{srcexe}');
  if CompareText(ExtractFileName(Src), '{#AppExe}') = 0 then
  begin
    Result := False;
    TempSetup := AddBackslash(GetTempDir) + '{#SetupName}.exe';
    Log('Restarting the setup from ' + TempSetup);
    if not Exec(ExpandConstant('{cmd}'),
      '/s /c "ping -n 3 127.0.0.1 >nul & copy /y "' + Src + '" "' + TempSetup + '" >nul & start "" "' + TempSetup + '""',
      '', SW_HIDE, ewNoWait, Code) then
      Log('Could not restart the setup: ' + SysErrorMessage(Code));
  end;
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
    { 10.x registered itself under this key; the setup's own entry replaces it. }
    RegDeleteKeyIncludingSubkeys(HKCU, LegacyUninstallKey);
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
  AppDir, OldDir: String;
begin
  if CurUninstallStep = usPostUninstall then
  begin
    AppDir := ExpandConstant('{app}');
    OldDir := ExpandConstant('{userappdata}\AkulavLauncher');
    { Never delete data when silent. }
    if (not UninstallSilent) and (HasLauncherData(AppDir) or HasLauncherData(OldDir)) then
      if MsgBox(CustomMessage('DeleteData'), mbConfirmation, MB_YESNO or MB_DEFBUTTON2) = IDYES then
      begin
        DeleteLauncherData(AppDir);
        DeleteLauncherData(OldDir);
      end;
    RemoveDir(AppDir);
  end;
end;
