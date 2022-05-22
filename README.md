# Spinup

A utility program to quickly audition (play) audio files with a terminal user interface.

![Screenshot of spinup in MacOS](assets/screenshot_macos_0.1.0.png?raw=true "MacOS Screenshot")

# Supported File Types

* .wav
* .ogg
* .mp3
* .flac

## Usage

* 'j' -- moves down in the list
* 'k' -- moves up in the list
* 'spacebar' -- plays a sample or navigates to the selected directory
* 'backspace' -- stops the current playback
* 'q' -- quits application

## Libraries Used

The major libraries involved are: 

* [tui](https://github.com/fdehau/tui-rs) -- Used for the terminal user interface. Initially was worried it took up too much CPU time, but once compiled for release it's fine.
* [kira](https://github.com/tesselode/kira) -- Used for audio file loading and playback. Awesome because it's simple and easy to use and provides handles to controll the playback of a sample.
* [symphonia](https://github.com/pdeljanov/Symphonia) -- Manually included to load a file and try to pull code information like bit depth, etc...

## Setup

```bash
$ cargo install spinup
```

## Tested Operating Systems

  * macOS Monterey (v12.4)
  * Linux (Manjaro Gnome)

## Feedback

Feedback is welcome on the project and feel free to open an issue or message me any requests.

## License

This program is free software: you can redistribute it and/or modify it under the terms of the GNU General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.

This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU General Public License for more details.

You should have received a copy of the GNU General Public License along with this program. If not, see <https://www.gnu.org/licenses/>. 

The full text is included in [LICENSE](LICENSE?raw=true "GPL3 License Text").
