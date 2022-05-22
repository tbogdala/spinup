// Copyright 2022 by Timothy Bogdala <tdb@animal-machine.com
// Source code is released under the GPL v3 license or greater, see 'LICENSE' for more details.

use std::error::Error;
use std::io;
use std::fs;
use std::path::{Path, PathBuf};
use std::ffi::OsString;

use clap::Parser;

use kira::sound::static_sound::{PlaybackState, StaticSoundHandle};
use tui::layout::Rect;
use tui::style::{Style, Color};
use tui::text::Spans;
use tui::widgets::{Borders, Block, Gauge, List, ListItem, ListState, Paragraph};

use kira::{
    manager::{
        AudioManager, AudioManagerSettings,
        backend::cpal::CpalBackend,
    },
    sound::static_sound::{StaticSoundData, StaticSoundSettings},
    tween::Tween,
};


// these are the supported fileformats from Kira / symphonia
const SUPPORTED_EXTS: [&str; 4] = ["wav", "ogg", "mp3", "flac"];

// this is the prefix used in the listitems for directories
const DIR_LISTITEM_PREFIX: &str = "<DIR> ";



/// Simple program to greet a person
#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    /// The starting directory to browse
    #[clap(short, long)]
    dir: Option<String>,
}


fn main() -> io::Result<()> {
    let args = Args::parse();

    // setup terminal
    crossterm::terminal::enable_raw_mode()?;
    let mut stdout = io::stdout();
    crossterm::execute!(
        stdout, 
        crossterm::terminal::EnterAlternateScreen, 
        crossterm::event::EnableMouseCapture
    )?;
    let backend = tui::backend::CrosstermBackend::new(stdout);
    let mut terminal = tui::Terminal::new(backend)?;

    let app_result = run_app(args, &mut terminal);

    crossterm::terminal::disable_raw_mode()?;
    crossterm::execute!(
        terminal.backend_mut(),
        crossterm::terminal::LeaveAlternateScreen,
        crossterm::event::DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    // print any error messages from running the app after we restored the terminal
    if let Err(e) = app_result {
        println!("There was an error while running the application: {}", e)
    }
    Ok(())
}

fn run_app<B: tui::backend::Backend>(args: Args, terminal: &mut tui::Terminal<B>) -> Result<(), Box<dyn Error>> {
    // initialize the audio system
    let mut audio_manager = AudioManager::<CpalBackend>::new(AudioManagerSettings::default())?;
    
    // build the initial application state
    let mut app_state = AppState::default();

    // use the optional starting directory if supplied, otherwise default to the current directory
    if let Some(starting_dir)  = args.dir {
        app_state.set_current_directory(&starting_dir);
    } else {
        app_state.set_current_directory(std::env::current_dir()?.to_str().unwrap());
    }
    app_state.update_file_names();
    app_state.select_list_item(0);

    
    let tick_rate = std::time::Duration::from_millis(66); // roughly 15fps
    let mut last_tick = std::time::Instant::now();
    loop {
        let current_tick = std::time::Instant::now();
        let tick_interval = current_tick.duration_since(last_tick);

        // update the played time of the sound, if currently playing
        if app_state.sound_state.is_playing() {
            app_state.sound_state.add_playtime(tick_interval);
        }

        // draw the interface
        terminal.draw(|f| ui(&mut app_state, f))?;

        // poll to see if we have an event based on our tick_rate if we're playing audio, otherwise 1s
        let timeout = if app_state.sound_state.is_playing() { tick_rate } else { std::time::Duration::from_secs(1) };
        if crossterm::event::poll(timeout)? {
            if let crossterm::event::Event::Key(key) = crossterm::event::read()? {
                // clear the error message before we do the next event.
                app_state.clear_error();

                match key.code {
                    crossterm::event::KeyCode::Char('q') => return Ok(()),
                    crossterm::event::KeyCode::Char('j') => {
                        app_state.next_list_item();
                        _ = app_state.update_selected_file_info();
                    }, 
                    crossterm::event::KeyCode::Char('k') => {
                        app_state.previous_list_item();
                        _ = app_state.update_selected_file_info();
                    }
                    crossterm::event::KeyCode::Backspace => {
                        if let Err(err) = app_state.sound_state.stop_sound() {
                            app_state.last_error_msg = format!("Playback Stop Error: {}", err.to_string());
                        }
                    },
                    crossterm::event::KeyCode::Char(' ') => {
                        if app_state.is_file_selected() {
                            if let Err(err) = play_selected_file(&mut app_state, &mut audio_manager) {
                                app_state.last_error_msg = format!("Playback Error: {}", err.to_string());
                            } 
                        } else if app_state.is_dir_selected() { 
                            if let Some(selected_dir_name) = app_state.get_selected_file_name() {
                                let snd_dir = Path::new(&app_state.current_directory_path);
                                match snd_dir.join(selected_dir_name).canonicalize() {
                                    Ok(new_dir) => {
                                        app_state.set_current_directory(&new_dir.to_str().unwrap());
                                        app_state.update_file_names();
                                        app_state.select_list_item(0);
                                    },
                                    Err(err) => app_state.last_error_msg = format!("Couldn't build path to selection: {}", err.to_string()),
                                }
                            }
                        }
                    },
                    
                    _ => {},
                }
            }
        }
        last_tick = current_tick;
    }
}

fn play_selected_file(app_state: &mut AppState, audio_manager: &mut AudioManager) -> Result<(), Box<dyn Error>>  {
    let sel_file_name = match app_state.get_selected_file_name() {
        Some(filename) => filename,
        None => return Ok(())
    };

    // build the file path out of the selected file and the directory
    let snd_dir = Path::new(&app_state.current_directory_path);
    let snd_path = snd_dir.join(sel_file_name);
    let sound_data = StaticSoundData::from_file(&snd_path, StaticSoundSettings::new())?;
    
    // cancel anything playing right before we queue our new file's data
    app_state.sound_state.stop_sound()?;

    // start playing
    let play_handle = audio_manager.play(sound_data.clone())?;

    
    app_state.sound_state.started_sound(play_handle, sound_data);

    Ok(())
}

fn ui<B: tui::backend::Backend>(app_state: &mut AppState, f: &mut tui::Frame<B>) {
    let whole_frame = f.size();

    // file list by default takes up the whole width and the info pane disabled
    let mut file_list_width = whole_frame.width;
    let mut show_info_pane = false;

    // decide if we're going to show the info pane -- should have at least twice
    // as much space as the minimum width.
    const WIDTH_INFO_PANE: u16 = 25;
    if file_list_width > WIDTH_INFO_PANE * 2 {
        if app_state.is_file_selected() {
            show_info_pane = true;
            file_list_width -= WIDTH_INFO_PANE;
        }
    }

    let mut chunks: Vec<Rect> = vec![
        // top menu line
        Rect {x: 0, y: 0, width:whole_frame.width, height: 1},

        // main file list
        Rect {x: 0, y: 1, width: file_list_width, height: whole_frame.height - 2},

        // error message / progress bar
        Rect {x: 0, y: whole_frame.height - 1, width: whole_frame.width, height: 1},
    ];

    // the 4th chunk will be present if the info pane is used
    if show_info_pane {
        chunks.push(Rect {x: file_list_width, y: 1, width: WIDTH_INFO_PANE, height: (whole_frame.height - 2).clamp(3, 5)});
    }

    // add the directories and files together
    let mut combined_filedir_list = app_state.directory_names.clone();
    let mut cloned_files = app_state.file_names.clone();
    combined_filedir_list.append(&mut cloned_files);

    // build the file list widget
    let file_list_items: Vec<ListItem> = combined_filedir_list.iter()
        .map(|name| {
            let new_li = ListItem::new(name.as_ref());
            if name.starts_with(DIR_LISTITEM_PREFIX) {
                new_li.style(Style::default().fg(Color::Blue))
            } else {
                new_li.style(Style::default())
            }
        })
        .collect();

    let list_block = Block::default()
        .title(format!("Dir: {}", app_state.current_directory_path))
        .borders(Borders::ALL);
    let list_widget = List::new(file_list_items)
        .block(list_block)
        .highlight_style(
            Style::default()
                .bg(Color::LightGreen)
                .add_modifier(tui::style::Modifier::BOLD),
        )
        .highlight_symbol(">> ");
    
    f.render_stateful_widget(list_widget, chunks[1], &mut app_state.file_list_state);

    // put a title bar at the top
    let title_widget = Paragraph::new("spinup:  (j)down | (k)up | (space) play or navigate dir | ((bksp)stop | (q)quit".as_ref())
        .alignment(tui::layout::Alignment::Left)
        .style(Style::default().add_modifier(tui::style::Modifier::BOLD));
    f.render_widget(title_widget, chunks[0]);

    // display errors if we have any
    if !app_state.last_error_msg.is_empty() {
        let err_widget = Paragraph::new(app_state.last_error_msg.as_ref())
            .style(tui::style::Style::default().fg(Color::Red));
        f.render_widget(err_widget, chunks[2]);
    } else if app_state.sound_state.is_playing() {
        let cur_ms = app_state.sound_state.play_time.as_millis();
        let total_ms = app_state.sound_state.play_duration.as_millis();
        let pct: f64 = cur_ms as f64 / total_ms as f64;
        if pct <= 1.0 { 
            let progress = Gauge::default()
                .gauge_style(Style::default().fg(Color::LightGreen).bg(Color::Black)).ratio(pct.clamp(0.0, 1.0));
            f.render_widget(progress, chunks[2]);
        }
    }
    
    // build the file info widget if it is used
    if show_info_pane && chunks.len() > 3 {
        let info_block = Block::default()
            .title("File Information")
            .borders(Borders::ALL);
        let mut info_text = vec![];
        if let Some(sr) = app_state.select_file_info.sample_rate {
            info_text.push(Spans::from(format!("Sample Rate: {}", sr)));
        }
        if let Some(bd) = app_state.select_file_info.bit_depth {
            info_text.push(Spans::from(format!("Bit Depth: {}", bd)));
        }
        if let Some(fl) = app_state.select_file_info.file_layout {
            let layout_str = match fl {
                symphonia::core::audio::Layout::Mono => "Mono",
                symphonia::core::audio::Layout::Stereo => "Stereo",
                symphonia::core::audio::Layout::TwoPointOne => "2.1",
                symphonia::core::audio::Layout::FivePointOne => "5.1",
            };

            info_text.push(Spans::from(format!("Layout: {}", layout_str)));
        }   
          
        let info_para = Paragraph::new(info_text)
            .block(info_block)
            .wrap(tui::widgets::Wrap {trim:true});
        f.render_widget(info_para, chunks[3]);
    }
}

#[derive(Default)]
struct AppState {
    needs_file_list_update: bool,
    current_directory_path: String,
    last_error_msg: String,

    file_names: Vec<String>,
    directory_names: Vec<String>,
    file_list_state: tui::widgets::ListState,
    select_file_info: SoundFileCodecData,

    sound_state: SoundState,
}

#[derive(Default)]
struct SoundState {
    sound: Option<StaticSoundHandle>,  // this may be the handle to the currently playing sound file
    sound_data: Option<StaticSoundData>, // this may be the data for the sound file playing
    play_time: std::time::Duration, // how long the file has been playing
    play_duration: std::time::Duration, // total duration of the sound
}

#[derive(Default, Clone, Copy)]
struct SoundFileCodecData {
    // information about the playing file
    sample_rate: Option<u32>,
    bit_depth: Option<u32>,
    file_layout: Option<symphonia::core::audio::Layout>,

}

impl SoundState {
    // stops the currently playing sound and resets the data structure.
    fn stop_sound(&mut self) -> Result<(), Box<dyn Error>> {
        if let Some(current_sound) = &mut self.sound {
            current_sound.stop(Tween::default())?;
            self.sound = None;
            self.sound_data = None;
            self.play_time = std::time::Duration::ZERO;
        }
        Ok(())
    }

    // update the data structure with the sound that just started playing
    fn started_sound(
        &mut self, 
        handle: StaticSoundHandle, 
        data: StaticSoundData,
    ) {
        self.play_duration = data.duration();
        self.sound = Some(handle);
        self.sound_data = Some(data);
        self.play_time = std::time::Duration::ZERO;
    }

    fn is_playing(&self) -> bool {
        if let Some(current_sound) = &self.sound {
            if current_sound.state() == PlaybackState::Playing {
                return true;
            }
        }
        false
    }

    fn add_playtime(&mut self, t: std::time::Duration) {
        if let Some(new_duration) = self.play_time.checked_add(t) {
            self.play_time = new_duration;
        }
    }
}


impl AppState {
    fn clear_error(&mut self) {
        self.last_error_msg.clear();
    }

    fn set_current_directory(&mut self, dir: &str) {
        self.current_directory_path = dir.to_string();
        self.needs_file_list_update = true;
    }

    fn update_selected_file_info(&mut self) -> Result<(), Box<dyn Error>>  {
        self.select_file_info.sample_rate = None;
        self.select_file_info.bit_depth = None;
        self.select_file_info.file_layout = None;

        // nothing to show for directories
        if !self.is_file_selected() {
            return Ok(());
        }

        // build the file path out of the selected file and the directory
        let sel_file_name = match self.get_selected_file_name() {
            Some(filename) => filename,
            None => return Ok(())
        };
        let snd_dir = Path::new(&self.current_directory_path);
        let snd_path = snd_dir.join(sel_file_name);
    
        // then pull up some extra data on the code and pass the status update to the app
        let probe = symphonia::default::get_probe();
        let mss = symphonia::core::io::MediaSourceStream::new(Box::new(std::fs::File::open(&snd_path)?), Default::default());
        let format_reader = probe
            .format(
                &Default::default(),
                mss,
                &Default::default(),
                &Default::default(),
            )?
            .format;
        let codec_params = &format_reader
            .default_track()
            .ok_or(kira::sound::FromFileError::NoDefaultTrack)?
            .codec_params;
    
        self.select_file_info.sample_rate = codec_params.sample_rate;
        self.select_file_info.bit_depth = codec_params.bits_per_sample;
        self.select_file_info.file_layout = codec_params.channel_layout;
    
        Ok(())
    }

    fn is_dir_selected(&self) -> bool {
        let sel_option = self.file_list_state.selected();
        if sel_option.is_none() {
            return false;
        }
        let sel_index = sel_option.unwrap();
        
        sel_index < self.directory_names.len()
    }

    fn is_file_selected(&self) -> bool {
        let sel_option = self.file_list_state.selected();
        if sel_option.is_none() {
            return false;
        }
        let sel_index = sel_option.unwrap();
        
        sel_index >= self.directory_names.len()
    }

    // returns the file name of the selected item in the list, or
    // the name of the directory without the prefix. Can return 
    // None if there is no selection.
    fn get_selected_file_name(&self) -> Option<String> {
        // the the index of the select file in the list
        let sel_option = self.file_list_state.selected();
        if sel_option.is_none() {
            return None;
        }
        
        let num_dirs = self.directory_names.len();
        let sel_index = sel_option.unwrap();
        
        if sel_index < num_dirs { // dir
            const PREFIX_LEN: usize = DIR_LISTITEM_PREFIX.len();
            let dir_with_prefix = &self.directory_names[sel_index];
            let dir_name = &dir_with_prefix[PREFIX_LEN..];
            Some(dir_name.to_string())
        } else { // file
            Some(self.file_names[sel_index - num_dirs].clone())
        }
    }

    fn update_file_names(&mut self) {
        if !self.needs_file_list_update {
            return;
        }

        let full_path = Path::new(&self.current_directory_path);
            
        self.directory_names.clear();
        match get_directories_in_dir(full_path) {
            Ok(os_names) => {
                let mut strings: Vec<String> = os_names.into_iter()
                    .filter_map(|osn| if let Ok(s) = osn.into_string() { Some(s) } else { None} )
                    .collect();
                
                strings.sort_by(|a, b| a.to_lowercase().cmp(&b.to_lowercase()));

                self.directory_names.append(&mut strings);
            }
            Err(e) => self.last_error_msg = format!("Failed to update directory list: {}", e)
        }

        self.file_names.clear();
        match get_supported_filenames_in_dir(full_path) {
            Ok(os_names) => {
                let mut strings: Vec<String> = os_names.into_iter()
                    .filter_map(|osn| if let Ok(s) = osn.into_string() { Some(s) } else { None} )
                    .collect();
                
                strings.sort_by(|a, b| a.to_lowercase().cmp(&b.to_lowercase()));
    
                self.file_names.append(&mut strings);
            }
            Err(e) => self.last_error_msg = format!("Failed to update file list: {}", e)
        }

        self.file_list_state = ListState::default();
        self.needs_file_list_update = false;        
    }

    fn select_list_item(&mut self, i: usize) {
        self.file_list_state.select(Some(i));
        _ = self.update_selected_file_info();
    }

    fn next_list_item(&mut self) {
        let i = match self.file_list_state.selected() {
            Some(i) => {
                let total_size = self.file_names.len() + self.directory_names.len();
                if i >= total_size - 1 {
                    0
                } else {
                    i + 1
                }
            }
            None => 0,
        };
        self.file_list_state.select(Some(i));
    }

    fn previous_list_item(&mut self) {
        let i = match self.file_list_state.selected() {
            Some(i) => {
                let total_size = self.file_names.len() + self.directory_names.len();
                if i == 0 {
                    total_size - 1
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        self.file_list_state.select(Some(i));
    }

    pub fn _unselect_list_item(&mut self) {
        self.file_list_state.select(None);
    }
}

fn get_directories_in_dir(dir_path: &Path) -> io::Result<Vec<OsString>> {
    let dir = fs::read_dir(dir_path)?;
    let mut filtered_paths: Vec<OsString> = dir.filter_map(Result::ok)
        .map(|e| e.path())
        .filter_map(|e| {
            if !e.is_dir() {
                return None;
            }
            if let Some(os_fn) = e.file_name() {
                let f = os_fn.to_str().unwrap();
                if !f.starts_with(".") {
                    Some(format!("{}{}", DIR_LISTITEM_PREFIX, f).into())
                } else {
                    None
                }
            } else {
                None
            }
        })
        .collect();

    if dir_path.parent().is_some() {
        let os_parent_str: OsString = format!("{}..", DIR_LISTITEM_PREFIX).into();
        filtered_paths.insert(0, os_parent_str);
    }

    return Ok(filtered_paths);
}

fn get_supported_filenames_in_dir(dir_path: &Path) -> io::Result<Vec<OsString>> {
    let paths = get_supported_files_in_dir(dir_path)?;
    let names = paths.iter().filter_map(|p| {if let Some(f) = p.file_name() { Some(f.to_os_string())} else {None}}).collect();
    Ok(names)
}

fn get_supported_files_in_dir(dir_path: &Path) -> io::Result<Vec<PathBuf>> {
    let dir = fs::read_dir(dir_path)?;
    let filtered_paths = dir.filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|e| e.is_file())
        .filter(|e| { 
            if let Some(os_fn) = e.file_name() {
                let f = os_fn.to_str().unwrap();
                if f.starts_with(".") {
                    return false;
                } 
            } else {
                return false;
            }
            if let Some(ext) = e.extension() {
                for supported in SUPPORTED_EXTS { 
                    if ext.eq_ignore_ascii_case(supported) {
                        return true;
                    } 
                } 
            }
            return false;
        })
        .collect();
    
    Ok(filtered_paths)
}