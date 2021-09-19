use std::{
	collections::HashMap,
	convert::TryInto,
	io::{Result, Write},
};

use byteorder::{LittleEndian, WriteBytesExt};
use encoding_rs::SHIFT_JIS;

use super::{
	frame,
	game::{self, MAX_PLAYERS, NUM_PORTS, Frames},
	item,
	parse::{PAYLOADS_EVENT_CODE, Event, PortId},
	slippi,
	ubjson,
};

type BE = byteorder::BigEndian;

const fn version(major: u8, minor: u8) -> slippi::Version {
	slippi::Version(major, minor, 0)
}

// TODO: write version-dependent sizes
fn payload_sizes(_ver: slippi::Version) -> HashMap<Event, u16> {
	let mut s = HashMap::new();
	s.insert(Event::GameStart, 420);
	s.insert(Event::FramePre, 63);
	s.insert(Event::FramePost, 72);
	s.insert(Event::GameEnd, 2);
	s.insert(Event::FrameStart, 8);
	s.insert(Event::Item, 42);
	s.insert(Event::FrameEnd, 8);
	s
}

fn write_shift_jis<W: Write>(w: &mut W, s: &String, max_size: usize, label: &str) -> Result<()> {
	let bytes = SHIFT_JIS.encode(s).0;
	if bytes.len() > max_size {
		panic!("{} > {} bytes", label, max_size);
	}
	w.write_all(&bytes)?;
	for _ in 0 .. (max_size - bytes.len()) {
		w.write_u8(0)?; // FIXME: this is dumb
	}
	Ok(())
}

fn game_start<W: Write>(w: &mut W, s: &game::Start, ver: slippi::Version) -> Result<()> {
	assert_eq!(ver, s.slippi.version);

	w.write_u8(Event::GameStart as u8)?;

	let u = s.unmapped.0;

	w.write_u8(ver.0)?;
	w.write_u8(ver.1)?;
	w.write_u8(ver.2)?;
	w.write_u8(0)?; // build number
	w.write_all(&s.bitfield)?;
	w.write_all(&u[0..2])?;
	w.write_u8(s.is_raining_bombs as u8)?;
	w.write_all(&u[2..3])?;
	w.write_u8(s.is_teams as u8)?;
	w.write_all(&u[3..5])?;
	w.write_i8(s.item_spawn_frequency)?;
	w.write_i8(s.self_destruct_score)?;
	w.write_all(&u[5..6])?;
	w.write_u16::<BE>(s.stage.0)?;
	w.write_u32::<BE>(s.timer)?;
	w.write_all(&u[6..21])?;
	w.write_all(&s.item_spawn_bitfield)?;
	w.write_all(&u[21..29])?;
	w.write_f32::<BE>(s.damage_ratio)?;
	w.write_all(&u[29..73])?;
	for p in &s.players {
		let u = p.unmapped.0;
		w.write_u8(p.character.0)?;
		w.write_u8(p.r#type.0)?;
		w.write_u8(p.stocks)?;
		w.write_u8(p.costume)?;
		w.write_all(&u[0..3])?;
		w.write_u8(p.team.map(|t| t.shade.0).unwrap_or(0))?;
		w.write_u8(p.handicap)?;
		w.write_u8(p.team.map(|t| t.color.0).unwrap_or(0))?;
		w.write_all(&u[3..5])?;
		w.write_u8(p.bitfield)?;
		w.write_all(&u[5..7])?;
		w.write_u8(p.cpu_level.unwrap_or(0))?;
		w.write_all(&u[7..11])?;
		w.write_f32::<BE>(p.offense_ratio)?;
		w.write_f32::<BE>(p.defense_ratio)?;
		w.write_f32::<BE>(p.model_scale)?;
		w.write_all(&u[11..15])?;
	}
	for _ in s.players.len() .. MAX_PLAYERS {
		w.write_all(&[0, 3])?; // 3 = player type "empty"
		w.write_all(&[0; 34])?;
	}
	w.write_u32::<BE>(s.random_seed)?;

	let empty_player_slots = 0 .. (NUM_PORTS - s.players.len());
	if ver >= version(1, 0) {
		for p in &s.players {
			w.write_u32::<BE>(p.ucf.unwrap().dash_back.map(|d| d.0).unwrap_or(0))?;
			w.write_u32::<BE>(p.ucf.unwrap().shield_drop.map(|d| d.0).unwrap_or(0))?;
		}
		for _ in empty_player_slots.clone() {
			w.write_all(&[0; 8])?;
		}
	}

	if ver >= version(1, 3) {
		for p in &s.players {
			write_shift_jis(w, &p.name_tag.as_ref().unwrap(), 16, "player.name_tag")?;
		}
		for _ in empty_player_slots.clone() {
			w.write_all(&[0; 16])?;
		}
	}

	if ver >= version(1, 5) {
		w.write_u8(s.is_pal.unwrap() as u8)?;
	}

	if ver >= version(2, 0) {
		w.write_u8(s.is_frozen_ps.unwrap() as u8)?;
	}

	if ver >= version(3, 7) {
		w.write_u8(s.scene.unwrap().minor)?;
		w.write_u8(s.scene.unwrap().major)?;
	}

	if ver >= version(3, 9) {
		for p in &s.players {
			let netplay = p.netplay.as_ref().unwrap();
			write_shift_jis(w, &netplay.name, 31, "player.netplay.name")?;
			write_shift_jis(w, &netplay.code, 10, "player.netplay.code")?;
		}
		for _ in empty_player_slots.clone() {
			w.write_all(&[0; 41])?;
		}
	}

	Ok(())
}

fn game_end<W: Write>(w: &mut W, e: &game::End, ver: slippi::Version) -> Result<()> {
	w.write_u8(Event::GameEnd as u8)?;
	w.write_u8(e.method.0)?;
	if ver >= version(2, 0) {
		w.write_u8(e.lras_initiator.unwrap().map(|p| p.into()).unwrap_or(u8::MAX))?;
	}
	Ok(())
}

fn frame_start<W: Write>(w: &mut W, s: &frame::Start, _ver: slippi::Version, frame_idx: i32) -> Result<()> {
	w.write_u8(Event::FrameStart as u8)?;
	w.write_i32::<BE>(frame_idx)?;
	w.write_u32::<BE>(s.random_seed)?;
	Ok(())
}

fn frame_pre<W: Write>(w: &mut W, p: &frame::Pre, ver: slippi::Version, id: PortId) -> Result<()> {
	w.write_u8(Event::FramePre as u8)?;
	w.write_i32::<BE>(id.index)?;
	w.write_u8(id.port as u8)?;
	w.write_u8(id.is_follower as u8)?;

	w.write_u32::<BE>(p.random_seed)?;
	w.write_u16::<BE>(p.state.into())?;
	w.write_f32::<BE>(p.position.x)?;
	w.write_f32::<BE>(p.position.y)?;
	w.write_f32::<BE>(p.direction.into())?;
	w.write_f32::<BE>(p.joystick.x)?;
	w.write_f32::<BE>(p.joystick.y)?;
	w.write_f32::<BE>(p.cstick.x)?;
	w.write_f32::<BE>(p.cstick.y)?;
	w.write_f32::<BE>(p.triggers.logical)?;
	w.write_u32::<BE>(p.buttons.logical.0)?;
	w.write_u16::<BE>(p.buttons.physical.0)?;
	w.write_f32::<BE>(p.triggers.physical.l)?;
	w.write_f32::<BE>(p.triggers.physical.r)?;

	if ver >= version(1, 2) {
		w.write_u8(p.raw_analog_x.unwrap())?;
	}

	if ver >= version(1, 4) {
		w.write_f32::<BE>(p.damage.unwrap())?;
	}

	Ok(())
}

fn frame_post<W: Write>(w: &mut W, p: &frame::Post, ver: slippi::Version, id: PortId) -> Result<()> {
	w.write_u8(Event::FramePost as u8)?;
	w.write_i32::<BE>(id.index)?;
	w.write_u8(id.port as u8)?;
	w.write_u8(id.is_follower as u8)?;

	w.write_u8(p.character.0)?;
	w.write_u16::<BE>(p.state.into())?;
	w.write_f32::<BE>(p.position.x)?;
	w.write_f32::<BE>(p.position.y)?;
	w.write_f32::<BE>(p.direction.into())?;
	w.write_f32::<BE>(p.damage)?;
	w.write_f32::<BE>(p.shield)?;
	w.write_u8(p.last_attack_landed.map(|a| a.0).unwrap_or(0))?;
	w.write_u8(p.combo_count)?;
	w.write_u8(p.last_hit_by.map(|p| p as u8).unwrap_or(u8::MAX))?;
	w.write_u8(p.stocks)?;

	if ver >= version(0, 2) {
		w.write_f32::<BE>(p.state_age.unwrap())?;
	}

	if ver >= version(2, 0) {
		let mut buf = [0u8; 8];
		buf.as_mut().write_u64::<LittleEndian>(p.flags.unwrap().0)?;
		w.write_all(&buf[0..5])?;
		w.write_f32::<BE>(p.misc_as.unwrap())?;
		w.write_u8(p.airborne.unwrap() as u8)?;
		w.write_u16::<BE>(p.ground.unwrap().0)?;
		w.write_u8(p.jumps.unwrap())?;
		w.write_u8(match p.l_cancel.unwrap() { Some(true) => 1, Some(false) => 2, _ => 0 })?;
	}

	if ver >= version(2, 1) {
		w.write_u8(p.hurtbox_state.unwrap().0)?;
	}

	if ver >= version(3, 5) {
		let vel = p.velocities.unwrap();
		w.write_f32::<BE>(if p.airborne.unwrap() { vel.autogenous.x } else { 0.0 })?;
		w.write_f32::<BE>(vel.autogenous.y)?;
		w.write_f32::<BE>(vel.knockback.x)?;
		w.write_f32::<BE>(vel.knockback.y)?;
		w.write_f32::<BE>(if p.airborne.unwrap() { 0.0 } else { vel.autogenous.x })?;
	}

	if ver >= version(3,8) {
		w.write_f32::<BE>(p.hitlag.unwrap())?;
	}

	Ok(())
}

fn item<W: Write>(w: &mut W, i: &item::Item, ver: slippi::Version, frame_idx: i32) -> Result<()> {
	w.write_u8(Event::Item as u8)?;
	w.write_i32::<BE>(frame_idx)?;

	w.write_u16::<BE>(i.r#type.0)?;
	w.write_u8(i.state.0)?;
	w.write_f32::<BE>(i.direction.map(|d| d.into()).unwrap_or(0.0))?;
	w.write_f32::<BE>(i.velocity.x)?;
	w.write_f32::<BE>(i.velocity.y)?;
	w.write_f32::<BE>(i.position.x)?;
	w.write_f32::<BE>(i.position.y)?;
	w.write_u16::<BE>(i.damage)?;
	w.write_f32::<BE>(i.timer)?;
	w.write_u32::<BE>(i.id)?;

	if ver >= version(3, 2) {
		w.write_all(&i.misc.unwrap())?;
	}

	if ver >= version(3, 6) {
		w.write_u8(i.owner.unwrap().map(|p| p as u8).unwrap_or(u8::MAX))?;
	}

	Ok(())
}

fn frame_end<W: Write>(w: &mut W, e: &frame::End, ver: slippi::Version, frame_idx: i32) -> Result<()> {
	w.write_u8(Event::FrameEnd as u8)?;
	w.write_i32::<BE>(frame_idx)?;
	if ver >= version(3, 7) {
		w.write_i32::<BE>(e.latest_finalized_frame.unwrap())?;
	}
	Ok(())
}

fn frames<W: Write, const N: usize>(w: &mut W, frames: &Vec<frame::Frame<N>>, ver: slippi::Version) -> Result<()> {
	let mut frame_idx = game::FIRST_FRAME_INDEX;
	for f in frames {
		if ver >= version(2, 2) {
			frame_start(w, f.start.as_ref().unwrap(), ver, frame_idx)?;
		}

		let mut port_idx = 0u8;
		for p in &f.ports {
			frame_pre(w, &p.leader.pre, ver, PortId::new(frame_idx, port_idx, false)?)?;
			if let Some(follower) = &p.follower {
				frame_pre(w, &follower.pre, ver, PortId::new(frame_idx, port_idx, true)?)?;
			}

			frame_post(w, &p.leader.post, ver, PortId::new(frame_idx, port_idx, false)?)?;
			if let Some(follower) = &p.follower {
				frame_post(w, &follower.post, ver, PortId::new(frame_idx, port_idx, true)?)?;
			}

			port_idx += 1;
		}

		if ver >= version(3, 0) {
			for i in f.items.as_ref().unwrap() {
				item(w, &i, ver, frame_idx)?;
			}
		}

		if ver >= version(3, 0) {
			frame_end(w, f.end.as_ref().unwrap(), ver, frame_idx)?;
		}

		frame_idx += 1;
	}
	Ok(())
}

pub fn unparse<W: Write>(w: &mut W, game: &game::Game) -> Result<()> {
	w.write_all(
		&[0x7b, 0x55, 0x03, 0x72, 0x61, 0x77, 0x5b, 0x24, 0x55, 0x23, 0x6c])?;
	w.write_u32::<BE>(0)?; // TODO: raw element size

	let payload_sizes = payload_sizes(game.start.slippi.version);
	w.write_u8(PAYLOADS_EVENT_CODE)?;
	w.write_u8((payload_sizes.len() * 3 + 1).try_into().unwrap())?; // see note in `parse::payload_sizes`
	for (event, size) in payload_sizes {
		w.write_u8(event as u8)?;
		w.write_u16::<BE>(size)?;
	}

	let ver = game.start.slippi.version;
	game_start(w, &game.start, ver)?;
	match &game.frames {
		Frames::P1(f) => frames(w, f, ver)?,
		Frames::P2(f) => frames(w, f, ver)?,
		Frames::P3(f) => frames(w, f, ver)?,
		Frames::P4(f) => frames(w, f, ver)?,
	};
	game_end(w, &game.end, ver)?;

	w.write_all(
		&[0x55, 0x08, 0x6d, 0x65, 0x74, 0x61, 0x64, 0x61, 0x74, 0x61, 0x7b])?;
	ubjson::unparse_map(w, &game.metadata_raw)?;
	w.write_all(&[0x7d])?; // closing brace for `metadata`
	w.write_all(&[0x7d])?; // closing brace for top-level map

	Ok(())
}
