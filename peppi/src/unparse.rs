use std::{
	collections::HashMap,
	convert::TryInto,
	io::{Result, Write},
};

use byteorder::{LittleEndian, WriteBytesExt};

use super::{
	frame,
	game::{self, Frames},
	item,
	parse::{PAYLOADS_EVENT_CODE, Event, PortId},
	slippi::{self, version as ver},
	ubjson,
};

type BE = byteorder::BigEndian;

fn payload_sizes(start: &game::Start) -> HashMap<Event, u16> {
	let v = start.slippi.version;
	let mut s = HashMap::new();
	s.insert(Event::GameStart, start.raw_bytes.len() as u16);
	s.insert(Event::GameEnd,
		if v >= ver(2, 0) {
			2
		} else {
			1
		}
	);
	s.insert(Event::FramePre,
		if v >= ver(1, 4) {
			63
		} else if v >= ver(1, 2) {
			59
		} else {
			58
		}
	);
	s.insert(Event::FramePost,
		if v >= ver(3, 8) {
			76
		} else if v >= ver(3, 5) {
			72
		} else if v >= ver(2, 1) {
			52
		} else if v >= ver(2, 0) {
			51
		} else if v >= ver(0, 2) {
			37
		} else {
			33
		}
	);
	if v >= ver(2, 2) {
		s.insert(Event::FrameStart, 8);
	}
	if v >= ver(3, 0) {
		s.insert(Event::FrameEnd,
			if v >= ver(3, 7) {
				8
			} else {
				4
			}
		);
	}
	if v >= ver(3, 0) {
		s.insert(Event::Item,
			if v >= ver(3, 6) {
				42
			} else if v >= ver(3, 2) {
				41
			} else {
				37
			}
		);
	}
	s
}

fn game_start<W: Write>(w: &mut W, s: &game::Start, v: slippi::Version) -> Result<()> {
	assert_eq!(v, s.slippi.version);
	w.write_u8(Event::GameStart as u8)?;
	w.write_all(&s.raw_bytes)
}

fn game_end<W: Write>(w: &mut W, e: &game::End, v: slippi::Version) -> Result<()> {
	w.write_u8(Event::GameEnd as u8)?;
	w.write_u8(e.method.0)?;
	if v >= ver(2, 0) {
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

fn frame_pre<W: Write>(w: &mut W, p: &frame::Pre, v: slippi::Version, id: PortId) -> Result<()> {
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

	if v >= ver(1, 2) {
		w.write_u8(p.raw_analog_x.unwrap())?;
	}

	if v >= ver(1, 4) {
		w.write_f32::<BE>(p.damage.unwrap())?;
	}

	Ok(())
}

fn frame_post<W: Write>(w: &mut W, p: &frame::Post, v: slippi::Version, id: PortId) -> Result<()> {
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

	if v >= ver(0, 2) {
		w.write_f32::<BE>(p.state_age.unwrap())?;
	}

	if v >= ver(2, 0) {
		let mut buf = [0u8; 8];
		buf.as_mut().write_u64::<LittleEndian>(p.flags.unwrap().0)?;
		w.write_all(&buf[0..5])?;
		w.write_f32::<BE>(p.misc_as.unwrap())?;
		w.write_u8(p.airborne.unwrap() as u8)?;
		w.write_u16::<BE>(p.ground.unwrap().0)?;
		w.write_u8(p.jumps.unwrap())?;
		w.write_u8(match p.l_cancel.unwrap() { Some(true) => 1, Some(false) => 2, _ => 0 })?;
	}

	if v >= ver(2, 1) {
		w.write_u8(p.hurtbox_state.unwrap().0)?;
	}

	if v >= ver(3, 5) {
		let vel = p.velocities.unwrap();
		w.write_f32::<BE>(if p.airborne.unwrap() { vel.autogenous.x } else { 0.0 })?;
		w.write_f32::<BE>(vel.autogenous.y)?;
		w.write_f32::<BE>(vel.knockback.x)?;
		w.write_f32::<BE>(vel.knockback.y)?;
		w.write_f32::<BE>(if p.airborne.unwrap() { 0.0 } else { vel.autogenous.x })?;
	}

	if v >= ver(3,8) {
		w.write_f32::<BE>(p.hitlag.unwrap())?;
	}

	Ok(())
}

fn item<W: Write>(w: &mut W, i: &item::Item, v: slippi::Version, frame_idx: i32) -> Result<()> {
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

	if v >= ver(3, 2) {
		w.write_all(&i.misc.unwrap())?;
	}

	if v >= ver(3, 6) {
		w.write_u8(i.owner.unwrap().map(|p| p as u8).unwrap_or(u8::MAX))?;
	}

	Ok(())
}

fn frame_end<W: Write>(w: &mut W, e: &frame::End, v: slippi::Version, frame_idx: i32) -> Result<()> {
	w.write_u8(Event::FrameEnd as u8)?;
	w.write_i32::<BE>(frame_idx)?;
	if v >= ver(3, 7) {
		w.write_i32::<BE>(e.latest_finalized_frame.unwrap())?;
	}
	Ok(())
}

fn frames<W: Write, const N: usize>(w: &mut W, frames: &Vec<frame::Frame<N>>, v: slippi::Version) -> Result<()> {
	let mut frame_idx = game::FIRST_FRAME_INDEX;
	for f in frames {
		if v >= ver(2, 2) {
			frame_start(w, f.start.as_ref().unwrap(), v, frame_idx)?;
		}

		let mut port_idx = 0u8;
		for p in &f.ports {
			frame_pre(w, &p.leader.pre, v, PortId::new(frame_idx, port_idx, false)?)?;
			if let Some(follower) = &p.follower {
				frame_pre(w, &follower.pre, v, PortId::new(frame_idx, port_idx, true)?)?;
			}

			frame_post(w, &p.leader.post, v, PortId::new(frame_idx, port_idx, false)?)?;
			if let Some(follower) = &p.follower {
				frame_post(w, &follower.post, v, PortId::new(frame_idx, port_idx, true)?)?;
			}

			port_idx += 1;
		}

		if v >= ver(3, 0) {
			for i in f.items.as_ref().unwrap() {
				item(w, &i, v, frame_idx)?;
			}
		}

		if v >= ver(3, 0) {
			frame_end(w, f.end.as_ref().unwrap(), v, frame_idx)?;
		}

		frame_idx += 1;
	}
	Ok(())
}

pub fn unparse<W: Write>(w: &mut W, game: &game::Game) -> Result<()> {
	w.write_all(
		&[0x7b, 0x55, 0x03, 0x72, 0x61, 0x77, 0x5b, 0x24, 0x55, 0x23, 0x6c])?;
	w.write_u32::<BE>(0)?; // TODO: raw element size

	let payload_sizes = payload_sizes(&game.start);
	w.write_u8(PAYLOADS_EVENT_CODE)?;
	w.write_u8((payload_sizes.len() * 3 + 1).try_into().unwrap())?; // see note in `parse::payload_sizes`
	for (event, size) in payload_sizes {
		w.write_u8(event as u8)?;
		w.write_u16::<BE>(size)?;
	}

	let v = game.start.slippi.version;
	game_start(w, &game.start, v)?;
	match &game.frames {
		Frames::P1(f) => frames(w, f, v)?,
		Frames::P2(f) => frames(w, f, v)?,
		Frames::P3(f) => frames(w, f, v)?,
		Frames::P4(f) => frames(w, f, v)?,
	};
	game_end(w, &game.end, v)?;

	w.write_all(
		&[0x55, 0x08, 0x6d, 0x65, 0x74, 0x61, 0x64, 0x61, 0x74, 0x61, 0x7b])?;
	ubjson::unparse_map(w, &game.metadata_raw)?;
	w.write_all(&[0x7d])?; // closing brace for `metadata`
	w.write_all(&[0x7d])?; // closing brace for top-level map

	Ok(())
}
