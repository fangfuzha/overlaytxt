//! OverlayTxt 基础示例
//!
//! 演示如何创建透明覆盖层窗口并持续推送弹幕，
//! 包括图文混排弹幕（内联生成测试图片 + 文字）。
//!
//! # 运行
//!
//! ```bash
//! RUST_LOG=info cargo run --example basic
//! ```
//!
//! # 退出方式
//!
//! - **Ctrl+C** — 终端信号
//! - **Alt+Esc** — 外部注册的全局热键
//! - 调用 `app.stop()` — 程序化退出

use overlaytxt::{InlineContent, OverlayTxt, OverlayTxtConfig};
use std::time::Duration;

// ── 外部实现的退出机制：Windows 全局热键 ──

#[link(name = "user32")]
unsafe extern "system" {
	fn RegisterHotKey(hwnd: isize, id: i32, fsModifiers: u32, vk: u32) -> i32;
	fn UnregisterHotKey(hwnd: isize, id: i32) -> i32;
}

fn register_alt_esc_exit() -> i32 {
	const HOTKEY_ID: i32 = 1;
	const MOD_ALT: u32 = 0x0001;
	const MOD_NOREPEAT: u32 = 0x4000;
	const VK_ESCAPE: u32 = 0x1B;
	unsafe {
		RegisterHotKey(0, HOTKEY_ID, MOD_ALT | MOD_NOREPEAT, VK_ESCAPE);
	}
	HOTKEY_ID
}

fn unregister_hotkey(id: i32) {
	unsafe {
		UnregisterHotKey(0, id);
	}
}

// ── 内联测试图片生成 ──

/// 生成一个渐变圆形的测试图片，输出 straight alpha RGBA 数据。
///
/// 图片为 48×48，中心为橙红色渐变，边缘半透明。
/// InlineContent::rgba_image 会自动将 straight alpha 转换为预乘 alpha。
fn generate_test_image(size: u32) -> (Vec<u8>, u32, u32) {
	let mut data = Vec::with_capacity((size * size * 4) as usize);
	let center = (size as f32 - 1.0) / 2.0;
	let radius = size as f32 / 2.0;

	for y in 0..size {
		for x in 0..size {
			let dx = x as f32 - center;
			let dy = y as f32 - center;
			let dist = (dx * dx + dy * dy).sqrt();

			if dist <= radius {
				// 圆形内：橙红渐变，半透明边缘
				let t = dist / radius;
				let sr = 255u8;
				let sg = (200.0 * (1.0 - t * 0.7)) as u8;
				let sb = (100.0 * (1.0 - t)) as u8;
				let sa = (200 - (t * 80.0) as u32).max(80) as u8;
				// 输出 straight alpha，不用预乘，InlineContent::rgba_image 会自动转换
				data.push(sr);
				data.push(sg);
				data.push(sb);
				data.push(sa);
			} else {
				// 圆形外：透明
				data.extend_from_slice(&[0, 0, 0, 0]);
			}
		}
	}
	(data, size, size)
}

fn main() {
	env_logger::init();
	let hotkey_id = register_alt_esc_exit();

	// ── 生成测试图片 ──
	let (img_rgba, img_w, img_h) = generate_test_image(48);

	// 配置弹幕
	let config = OverlayTxtConfig {
		track_height: 40.0,
		default_font_size: 28.0,
		default_speed: 150.0,
		default_color: [255, 255, 255, 255],
		// 50% 显示区域：弹幕仅在上半屏显示
		display_area_percent: 0.5,
		background_enabled: false,
		background_opacity: 0.3,
	};

	let mut app = match OverlayTxt::new(config) {
		Ok(app) => app,
		Err(e) => {
			eprintln!("Failed to create OverlayTxt: {}", e);
			unregister_hotkey(hotkey_id);
			return;
		}
	};

	if let Err(e) = app.start() {
		eprintln!("Failed to start OverlayTxt: {}", e);
		unregister_hotkey(hotkey_id);
		return;
	}

	println!("OverlayTxt 已启动！");
	println!("弹幕窗口将覆盖整个屏幕。");
	println!("按 Ctrl+C 或 Alt+Esc 退出。\n");

	// ── 发送初始化弹幕 ──
	app.send_text("欢迎使用 OverlayTxt！");
	app.send_text_custom(
		"Direct2D + DirectWrite + DComp + 内联图片",
		Some(24.0),
		Some([255, 200, 100, 255]),
		Some(180.0),
	);
	app.send_text_custom("高性能弹幕渲染", Some(36.0), Some([100, 255, 100, 255]), Some(120.0));

	// 图文混排弹幕：文字 + 内联图片 + 文字
	app.send_inline(
		vec![
			InlineContent::text("图文混排 ", 28.0, [255; 4]),
			InlineContent::rgba_image(img_w, img_h, &img_rgba),
			InlineContent::text(" 内联图片", 28.0, [255; 4]),
		],
		Some(120.0),
	);

	// ── 持续推送弹幕（模拟实时弹幕流） ──
	let demo_messages = [
		("😀你好世界", [255, 255, 255, 255]),
		("主播好厉害！", [255, 200, 100, 255]),
		("666666", [100, 255, 100, 255]),
		("233333", [200, 200, 255, 255]),
		("哈哈哈哈哈", [255, 100, 100, 255]),
		("前方高能！", [255, 255, 0, 255]),
		("这也太好看了吧", [255, 150, 255, 255]),
		("打卡打卡", [150, 255, 150, 255]),
		("笑死我了", [255, 200, 200, 255]),
		("太强了！", [200, 200, 100, 255]),
		("期待下一期", [100, 200, 255, 255]),
		("冲冲冲！！！", [255, 100, 200, 255]),
		("来了来了", [200, 255, 255, 255]),
		("点赞投币收藏", [255, 215, 0, 255]),
		("优秀优秀", [180, 180, 255, 255]),
	];

	let mut index = 0;
	while app.is_running() {
		if check_hotkey_pressed() {
			println!("检测到 Alt+Esc，退出中...");
			app.stop();
			break;
		}

		let (msg, color) = demo_messages[index % demo_messages.len()];
		let font_size = 24.0 + (index as f32 % 3.0) * 4.0;
		let speed = 120.0 + (index as f32 % 5.0) * 20.0;

		// 每隔几条弹幕发送一条图文混排弹幕
		if index % 5 == 0 {
			app.send_inline(
				vec![
					InlineContent::text(" 🚀", font_size, color),
					InlineContent::rgba_image(img_w, img_h, &img_rgba),
					InlineContent::text(" 图片在这 ", font_size, color),
					InlineContent::rgba_image(img_w, img_h, &img_rgba),
					InlineContent::text(" 再来一张 ", font_size, color),
				],
				Some(speed),
			);

			// 同时补一条纯文字弹幕
			app.send_text_custom(msg, Some(font_size), Some(color), Some(speed));
		} else {
			app.send_text_custom(msg, Some(font_size), Some(color), Some(speed));
		}
		index += 1;

		let delay = Duration::from_millis(800 + (index as u64 % 7) * 100);
		std::thread::sleep(delay);
	}

	unregister_hotkey(hotkey_id);
	println!("OverlayTxt 已退出。");
}

/// 检查 WM_HOTKEY 消息。
fn check_hotkey_pressed() -> bool {
	const WM_HOTKEY: u32 = 0x0312;
	const PM_REMOVE: u32 = 0x0001;
	type HWND = isize;
	type UINT = u32;
	type WPARAM = usize;
	type LPARAM = isize;
	type BOOL = i32;

	#[link(name = "user32")]
	unsafe extern "system" {
		fn PeekMessageW(
			lpMsg: *mut MSG,
			hWnd: HWND,
			wMsgFilterMin: UINT,
			wMsgFilterMax: UINT,
			wRemoveMsg: UINT,
		) -> BOOL;
	}

	#[repr(C)]
	#[allow(non_snake_case)]
	struct MSG {
		hwnd: HWND,
		message: UINT,
		wParam: WPARAM,
		lParam: LPARAM,
		time: u32,
		pt_x: i32,
		pt_y: i32,
	}

	unsafe {
		let mut msg = std::mem::zeroed::<MSG>();
		let has_msg = PeekMessageW(&mut msg, 0, WM_HOTKEY, WM_HOTKEY, PM_REMOVE) != 0;
		has_msg && msg.message == WM_HOTKEY
	}
}
