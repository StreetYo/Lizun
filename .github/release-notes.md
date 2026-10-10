Early prototype for Windows 10/11 (x64).

## Which file to download

| File | For |
|---|---|
| `Lizun_*_x64-setup.exe` | **Most people.** Installs for your user, no admin rights needed, adds Lizun to the Start menu and to Apps for uninstalling. |
| `Lizun_*_x64_en-US.msi` | Installing for all users or through Group Policy, Intune and similar tools. Needs admin rights. |
| `Lizun_*_x64_portable.zip` | Trying Lizun without installing: unzip anywhere and run `Lizun.exe`. Settings and installed widgets are still kept in your user profile. |
| `SHA256SUMS.txt` | Checksums to verify the downloads (`Get-FileHash <file>` in PowerShell). |

Install only one of the installers. The installers set up the Microsoft WebView2 runtime if it
is missing; the portable version needs it already installed (Windows 11 always has it).

The files are not code-signed yet, so Windows SmartScreen may warn about them: click
**More info**, then **Run anyway**.

After starting, the island appears at the top of the screen and Lizun has an icon in the tray.
What works so far is listed in the [README](https://github.com/StreetYo/Lizun#what-works-today-windows).
