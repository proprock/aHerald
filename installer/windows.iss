#ifndef AppVersion
  #error AppVersion is required
#endif
#ifndef PackageDir
  #error PackageDir is required
#endif
#ifndef InstallerAppId
  #define InstallerAppId "aHerald.Native.User"
  #define OutputSuffix ""
#else
  #define OutputSuffix "-test"
#endif

[Setup]
AppId={#InstallerAppId}
AppName=aHerald
AppVersion={#AppVersion}
AppPublisher=proprock
AppPublisherURL=https://github.com/proprock/aHerald
DefaultDirName={localappdata}\Programs\aHerald
DefaultGroupName=aHerald
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0
DisableProgramGroupPage=yes
DisableDirPage=no
UsePreviousTasks=yes
CloseApplications=no
RestartApplications=no
UninstallDisplayIcon={app}\aherald.exe
OutputDir=..\dist
OutputBaseFilename=aherald-v{#AppVersion}-windows-x64-setup{#OutputSuffix}
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
WizardSizePercent=120,140
LicenseFile=..\LICENSE

[Tasks]
Name: codex; Description: "Configure Codex hooks for this user"; GroupDescription: "Connect aHerald to:"; Flags: unchecked
Name: claude; Description: "Configure Claude Code hooks for this user"; GroupDescription: "Connect aHerald to:"; Flags: unchecked
Name: usagecodex; Description: "Show Codex usage"; GroupDescription: "Optional usage display:"; Flags: unchecked
Name: usageclaude; Description: "Show Claude usage"; GroupDescription: "Optional usage display:"; Flags: unchecked
Name: contextcodex; Description: "Codex context warning (250k tokens)"; GroupDescription: "Session warnings (existing thresholds are preserved):"; Flags: unchecked
Name: contextclaude; Description: "Claude context warning (400k tokens)"; GroupDescription: "Session warnings (existing thresholds are preserved):"; Flags: unchecked
Name: idlecodex; Description: "Codex idle warning (25 minutes)"; GroupDescription: "Session warnings (existing thresholds are preserved):"; Flags: unchecked
Name: idleclaude; Description: "Claude idle warning (55 minutes)"; GroupDescription: "Session warnings (existing thresholds are preserved):"; Flags: unchecked

[Files]
Source: "{#PackageDir}\aherald.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#PackageDir}\assets\*"; DestDir: "{app}\assets"; Flags: ignoreversion recursesubdirs createallsubdirs
Source: "{#PackageDir}\docs\*"; DestDir: "{app}\docs"; Flags: ignoreversion recursesubdirs createallsubdirs
Source: "{#PackageDir}\examples\*"; DestDir: "{app}\examples"; Flags: ignoreversion recursesubdirs createallsubdirs
Source: "{#PackageDir}\aHerald.example.toml"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#PackageDir}\LICENSE"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#PackageDir}\README.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#PackageDir}\CHANGELOG.md"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{group}\aHerald"; Filename: "{app}\aherald.exe"; Parameters: "--resident"
Name: "{group}\Installation guide"; Filename: "{app}\docs\installation.md"
Name: "{group}\Uninstall aHerald"; Filename: "{uninstallexe}"

[Run]
Filename: "{app}\aherald.exe"; Parameters: "--resident"; Description: "Launch aHerald"; Flags: nowait postinstall skipifsilent; Check: CanLaunchResident

[Code]
var
  SetupFailed: Boolean;
  SettingsTasksLoadedFor: String;

procedure InitializeWizard;
var
  UsageNote: TNewStaticText;
begin
  WizardForm.TasksList.Height := WizardForm.TasksList.Height - ScaleY(78);
  UsageNote := TNewStaticText.Create(WizardForm);
  UsageNote.Parent := WizardForm.SelectTasksPage;
  UsageNote.AutoSize := False;
  UsageNote.WordWrap := True;
  UsageNote.SetBounds(WizardForm.TasksList.Left,
    WizardForm.TasksList.Top + WizardForm.TasksList.Height + ScaleY(6),
    WizardForm.TasksList.Width, ScaleY(72));
  UsageNote.Caption := 'Usage uses your signed-in account. Claude defaults to auto: local Claude login, then statusLine. Upgrades keep the configured Claude source.' + #13#10 +
    'Warnings require a running resident. Enabling Claude context configures the statusLine wrapper and preserves your existing status line.';
end;

function SetupParams: String;
begin
  Result := '';
  if WizardIsTaskSelected('codex') then Result := Result + ' --harness codex';
  if WizardIsTaskSelected('claude') then Result := Result + ' --harness claude';
  if Result = '' then Result := '--unsetup'
  else Result := '--setup' + Result;
end;

function RunSetup(const Filename, Params: String): String;
var
  Code, I: Integer;
  Output: TExecOutput;
begin
  Result := '';
  if not ExecAndCaptureOutput(Filename, Params, '', SW_HIDE, ewWaitUntilTerminated, Code, Output) then
    Result := 'Could not start aHerald setup: ' + SysErrorMessage(Code)
  else begin
    for I := 0 to GetArrayLength(Output.StdOut) - 1 do Log(Output.StdOut[I]);
    for I := 0 to GetArrayLength(Output.StdErr) - 1 do Log(Output.StdErr[I]);
    if Code <> 0 then begin
      Result := 'aHerald could not configure the selected hooks.';
      for I := 0 to GetArrayLength(Output.StdErr) - 1 do
        Result := Result + #13#10 + Output.StdErr[I];
    end;
    if Output.Error then Result := 'Could not capture the aHerald setup report; inspect the setup log.';
  end;
end;

function OpenResidentProcess(Access: DWORD; Inherit: BOOL; PID: DWORD): THandle;
  external 'OpenProcess@kernel32.dll stdcall';
function ResidentImageName(Process: THandle; Flags: DWORD; Name: String; var Size: DWORD): BOOL;
  external 'QueryFullProcessImageNameW@kernel32.dll stdcall';
function WaitResident(Process: THandle; Milliseconds: DWORD): DWORD;
  external 'WaitForSingleObject@kernel32.dll stdcall';
function CloseResidentHandle(Process: THandle): BOOL;
  external 'CloseHandle@kernel32.dll stdcall';

function StopInstalledResident: String;
var
  Filename, Report, PIDText, Image: String;
  Code, I, Start, PID: Integer;
  ImageSize: DWORD;
  Process: THandle;
  Output: TExecOutput;
begin
  Result := '';
  Filename := ExpandConstant('{app}\aherald.exe');
  if not FileExists(Filename) then Exit;
  if not ExecAndCaptureOutput(Filename, '--status', '', SW_HIDE,
    ewWaitUntilTerminated, Code, Output) then begin
    Result := 'Could not inspect the installed aHerald. Close it and retry.';
    Exit;
  end;
  if (Code <> 0) or Output.Error then begin
    Result := 'Could not inspect the installed aHerald. Close it and retry.';
    Exit;
  end;
  Report := '';
  for I := 0 to GetArrayLength(Output.StdOut) - 1 do
    Report := Report + Output.StdOut[I];
  StringChangeEx(Report, ' ', '', True);
  if Report = '{"running":false}' then Exit;
  Start := Pos('"pid":', Report);
  if Start = 0 then begin
    Result := 'The installed aHerald returned an unknown status. Close it and retry.';
    Exit;
  end;
  I := Start + Length('"pid":');
  PIDText := '';
  while I <= Length(Report) do begin
    if (Report[I] < '0') or (Report[I] > '9') then Break;
    PIDText := PIDText + Report[I];
    I := I + 1;
  end;
  PID := StrToIntDef(PIDText, 0);
  if PID <= 0 then begin
    Result := 'The installed aHerald returned an invalid process ID.';
    Exit;
  end;
  // Query only and synchronize: never request process termination rights.
  Process := OpenResidentProcess($00101000, False, PID);
  if Process = 0 then begin
    Result := 'Could not inspect the running aHerald process. Close it and retry.';
    Exit;
  end;
  try
    if WaitResident(Process, 0) = 0 then Exit;
    ImageSize := 32768;
    SetLength(Image, ImageSize);
    if not ResidentImageName(Process, 0, Image, ImageSize) then begin
      Result := 'Could not verify the running aHerald path. Close it and retry.';
      Exit;
    end;
    SetLength(Image, ImageSize);
    if CompareText(ExpandFileName(Image), ExpandFileName(Filename)) <> 0 then begin
      Log('Leaving the resident from another installation running.');
      Exit;
    end;
    Log('Requesting graceful shutdown of the installed aHerald.');
    if not ExecAndCaptureOutput(Filename, '--quit', '', SW_HIDE,
      ewWaitUntilTerminated, Code, Output) then begin
      Result := 'Could not request aHerald shutdown. Close it and retry.';
      Exit;
    end;
    if (Code <> 0) or Output.Error then begin
      Result := 'aHerald shutdown failed. Close it and retry.';
      Exit;
    end;
    if WaitResident(Process, 10000) <> 0 then
      Result := 'aHerald did not exit within 10 seconds. Close it and retry. No files were replaced.'
    else
      Log('The installed aHerald process has exited.');
  finally
    CloseResidentHandle(Process);
  end;
end;

function SettingsParams: String;
begin
  if WizardIsTaskSelected('usagecodex') then Result := ' --codex-usage true'
  else Result := ' --codex-usage false';
  if WizardIsTaskSelected('usageclaude') then Result := Result + ' --claude-usage true'
  else Result := Result + ' --claude-usage false';
  if WizardIsTaskSelected('contextcodex') then Result := Result + ' --codex-context true'
  else Result := Result + ' --codex-context false';
  if WizardIsTaskSelected('contextclaude') then Result := Result + ' --claude-context true'
  else Result := Result + ' --claude-context false';
  if WizardIsTaskSelected('idlecodex') then Result := Result + ' --codex-idle true'
  else Result := Result + ' --codex-idle false';
  if WizardIsTaskSelected('idleclaude') then Result := Result + ' --claude-idle true'
  else Result := Result + ' --claude-idle false';
  Result := Result + ' --settings-file "' + ExpandConstant('{app}\config.toml') + '"';
  Result := Result + ' --setup-executable "' + ExpandConstant('{app}\aherald.exe') + '"';
end;

function ReadTaskSetting(const Lines: TArrayOfString; const Key, Task: String): Boolean;
var
  I: Integer;
begin
  Result := False;
  for I := 0 to GetArrayLength(Lines) - 1 do begin
    if Lines[I] = Key + '=true' then begin
      WizardSelectTasks(Task);
      Result := True;
      Exit;
    end;
    if Lines[I] = Key + '=false' then begin
      WizardSelectTasks('!' + Task);
      Result := True;
      Exit;
    end;
  end;
end;

function LoadSettingsTasks: String;
var
  Code, I: Integer;
  Output: TExecOutput;
  ConfigFile: String;
begin
  Result := '';
  ConfigFile := ExpandConstant('{app}\config.toml');
  if SettingsTasksLoadedFor = ConfigFile then Exit;
  ExtractTemporaryFile('aherald.exe');
  if not ExecAndCaptureOutput(ExpandConstant('{tmp}\aherald.exe'),
    '--setup-settings --settings-file "' + ConfigFile + '"', '', SW_HIDE,
    ewWaitUntilTerminated, Code, Output) then begin
    Result := 'Could not read the existing settings: ' + SysErrorMessage(Code);
    Exit;
  end;
  if (Code <> 0) or Output.Error then begin
    Result := 'Could not read the existing configuration. Repair it or choose another installation directory.';
    for I := 0 to GetArrayLength(Output.StdErr) - 1 do
      Result := Result + #13#10 + Output.StdErr[I];
    Exit;
  end;
  if not ReadTaskSetting(Output.StdOut, 'codex_usage', 'usagecodex') or
     not ReadTaskSetting(Output.StdOut, 'claude_usage', 'usageclaude') or
     not ReadTaskSetting(Output.StdOut, 'codex_context', 'contextcodex') or
     not ReadTaskSetting(Output.StdOut, 'claude_context', 'contextclaude') or
     not ReadTaskSetting(Output.StdOut, 'codex_idle', 'idlecodex') or
     not ReadTaskSetting(Output.StdOut, 'claude_idle', 'idleclaude') then begin
    Result := 'The application returned an incomplete settings report.';
    Exit;
  end;
  SettingsTasksLoadedFor := ConfigFile;
end;

procedure CurPageChanged(CurPageID: Integer);
var
  Error: String;
begin
  if (CurPageID = wpSelectTasks) and not WizardSilent then begin
    Error := LoadSettingsTasks;
    if Error <> '' then SuppressibleMsgBox(Error, mbError, MB_OK, IDOK);
  end;
end;

function PrepareToInstall(var NeedsRestart: Boolean): String;
begin
  // Validate the selected hook files with the new binary before replacing any files.
  // This is a dry run from TEMP; the real binding uses {app} after extraction.
  ExtractTemporaryFile('aherald.exe');
  // Explicit /TASKS is a complete selection. Without it, silent upgrades read config.
  if WizardSilent and (ExpandConstant('{param:TASKS|__unspecified__}') = '__unspecified__') then begin
    Result := LoadSettingsTasks;
    if Result <> '' then Exit;
  end;
  Result := RunSetup(ExpandConstant('{tmp}\aherald.exe'), SetupParams + SettingsParams);
  if Result = '' then Result := StopInstalledResident;
end;

procedure CurStepChanged(CurStep: TSetupStep);
var
  Error: String;
begin
  if CurStep = ssPostInstall then begin
    Error := RunSetup(ExpandConstant('{app}\aherald.exe'), SetupParams + SettingsParams + ' --apply');
    SetupFailed := Error <> '';
    if SetupFailed then begin
      Log(Error);
      WizardForm.FinishedLabel.Caption := Error + #13#10 + 'The application files are installed. Repair the configuration and run this installer again.';
      SuppressibleMsgBox(Error, mbError, MB_OK, IDOK);
    end;
  end;
end;

function CanLaunchResident: Boolean;
begin
  Result := not SetupFailed;
end;

function GetCustomSetupExitCode: Integer;
begin
  if SetupFailed then Result := 10 else Result := 0;
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
var
  Error: String;
begin
  if CurUninstallStep = usUninstall then begin
    Error := RunSetup(ExpandConstant('{app}\aherald.exe'), '--unsetup');
    if Error = '' then Error := StopInstalledResident;
    if Error = '' then
      Error := RunSetup(ExpandConstant('{app}\aherald.exe'), '--unsetup --apply');
    if Error <> '' then begin
      SuppressibleMsgBox(Error + #13#10 + 'Uninstall was cancelled; the application files were kept.', mbError, MB_OK, IDOK);
      Abort;
    end;
  end;
end;
