use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::HBRUSH;
use windows::Win32::System::Com::*;
use windows::Win32::System::LibraryLoader::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::*;

use crate::danmaku::DanmakuManager;
use crate::danmaku::InlineContent;
use crate::danmaku::ProcessedSegment;
use crate::renderer::DcompRenderer;

/// 从主线程发送到渲染线程的命令。
///
/// 通过 `mpsc` 通道传递，由 [`OverlayTxt`](crate::OverlayTxt) 的 `send_*` 方法
/// 在任意线程发送，在渲染线程的消息循环中消费。
#[derive(Clone)]
pub enum RenderCommand {
	/// 添加一条新弹幕到渲染队列（纯文字）。
	AddDanmaku {
		/// 弹幕文字内容（UTF-8）
		text: String,
		/// 字体大小，单位：像素
		font_size: f32,
		/// RGBA 颜色，每通道 0-255
		color: [u8; 4],
		/// 水平移动速度，单位：像素/秒
		speed: f32,
	},
	/// 添加一条图文混排弹幕。
	AddInlineDanmaku {
		/// 内容段列表（Text/Image 交错排列）
		segments: Vec<InlineContent>,
		/// 水平移动速度，单位：像素/秒
		speed: f32,
	},
	/// 设置弹幕显示区域比例，实时更新轨道和渲染背景。
	SetDisplayAreaPercent(f32),
	/// 设置弹幕轨道高度，实时重新分配轨道。
	SetTrackHeight(f32),
	/// 设置是否在弹幕显示区域绘制半透明背景。
	SetBackgroundEnabled(bool),
	/// 设置弹幕显示区域的背景透明度。
	SetBackgroundOpacity(f32),
	/// 清空所有活跃弹幕。
	ClearAll,
	/// 暂停弹幕动画（停止移动，保持当前位置）。
	Pause,
	/// 恢复弹幕动画。
	Resume,
	/// 设置最大并发弹幕数量。
	SetMaxDanmaku(usize),
	/// 通知渲染线程退出消息循环。
	Quit,
}

/// 窗口创建参数。
///
/// 由 [`run_window`] 使用，在渲染线程中创建全屏覆盖窗口。
pub struct WindowConfig {
	/// 屏幕宽度（物理像素）
	pub screen_width: u32,
	/// 屏幕高度（物理像素）
	pub screen_height: u32,
	/// 每个弹幕轨道的高度（像素）
	pub track_height: f32,
	/// 弹幕显示区域占屏幕高度的比例（0.0 ~ 1.0）
	pub display_area_percent: f32,
	/// 是否在弹幕显示区域绘制半透明背景
	pub background_enabled: bool,
	/// 背景透明度（0.0 ~ 1.0）
	pub background_opacity: f32,
}

/// 窗口创建、消息循环调度（在渲染线程上运行）。
///
/// 内部执行：
/// 1. 初始化 COM
/// 2. 创建全屏顶层覆盖窗口（`WS_EX_LAYERED | WS_EX_TRANSPARENT`）
/// 3. 初始化 DComp 渲染器
/// 4. 初始化弹幕管理器
/// 5. 进入基于 `PeekMessageW` 的非阻塞消息循环（≈60 FPS）
///
/// 窗口关闭条件：
/// - 收到 `WM_DESTROY` 或 `WM_QUIT`
/// - 收到 `RenderCommand::Quit`
pub fn run_window(
	config: WindowConfig,
	command_rx: Arc<Mutex<mpsc::Receiver<RenderCommand>>>,
	paused: Arc<AtomicBool>,
) -> Result<()> {
	unsafe {
		// ── 初始化 COM ──
		let hr = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
		if !hr.is_ok() {
			log::warn!("CoInitializeEx warning: HRESULT={:08X}", hr.0);
		}

		// ── 创建窗口 ──
		let hwnd = create_window(config.screen_width, config.screen_height)?;

		// ── 初始化渲染器 ──
		let mut renderer = DcompRenderer::new(
			hwnd,
			config.screen_width,
			config.screen_height,
			config.background_enabled,
			config.background_opacity,
		)?;

		// ── 弹幕管理器 ──
		let mut manager = DanmakuManager::new(
			config.screen_width as f32,
			config.screen_height as f32,
			config.track_height,
			config.display_area_percent,
		);

		// ── 定时器 (≈60 FPS) ──
		let timer_id: usize = 1;
		SetTimer(Some(hwnd), timer_id, 16, None);

		let mut last_time = std::time::Instant::now();
		let mut msg = MSG::default();
		let mut running = true;

		while running {
			let has_message = PeekMessageW(&mut msg, Some(hwnd), 0, 0, PM_REMOVE).as_bool();

			if has_message {
				match msg.message {
					WM_DESTROY | WM_QUIT => {
						running = false;
						continue;
					}
					WM_TIMER if msg.wParam.0 == timer_id => {
						let now = std::time::Instant::now();
						let dt = (now - last_time).as_secs_f32().min(0.1);
						last_time = now;

						// 处理新命令
						if let Ok(rx) = command_rx.lock() {
							while let Ok(cmd) = rx.try_recv() {
								match cmd {
									RenderCommand::AddDanmaku { text, font_size, color, speed } => {
										let text_utf16: Vec<u16> = text.encode_utf16().collect();
										let text_width = renderer
											.measure_text_width(&text_utf16, font_size)
											.unwrap_or(100.0);
										manager.add_text(
											text_utf16, font_size, color, speed, text_width,
										);
									}
									RenderCommand::AddInlineDanmaku { segments, speed } => {
										let mut processed = Vec::new();
										let mut total_width = 0.0_f32;

										// 先扫描所有文字段，取最大字号作为图片的缩放目标高度
										let target_img_height = segments
											.iter()
											.filter_map(|seg| match seg {
												InlineContent::Text { font_size, .. } => {
													Some(*font_size)
												}
												InlineContent::Image { .. } => None,
											})
											.fold(0.0_f32, f32::max)
											.max(16.0); // 最低 16px 保底

										for seg in segments {
											match seg {
												InlineContent::Text { text, font_size, color } => {
													let text_utf16: Vec<u16> =
														text.encode_utf16().collect();
													let w = renderer
														.measure_text_width(&text_utf16, font_size)
														.unwrap_or(100.0);
													total_width += w;
													processed.push(ProcessedSegment::Text {
														text: text_utf16,
														font_size,
														color,
														width: w,
													});
												}
												InlineContent::Image { rgba, width, height } => {
													if let Ok(bitmap) = renderer
														.create_bitmap_from_rgba(
															&rgba, width, height,
														) {
														// 等比缩放：图片高度对齐文字高度，宽度按比例缩放
														let scale =
															target_img_height / height as f32;
														let w = width as f32 * scale;
														let h = target_img_height;
														// DWrite 文字在字形上方留有内部 leading，图片下移以视觉对齐
														let y_offset = target_img_height * 0.15;
														total_width += w;
														processed.push(ProcessedSegment::Image {
															bitmap,
															width: w,
															height: h,
															y_offset,
														});
													}
												}
											}
										}
										if !processed.is_empty() {
											manager.add_inline(processed, speed, total_width);
										}
									}
									RenderCommand::SetDisplayAreaPercent(percent) => {
										let display_h =
											(config.screen_height as f32 * percent).ceil() as u32;
										renderer.set_display_area_height(display_h);
										manager.set_display_area_percent(percent);
									}
									RenderCommand::SetTrackHeight(track_height) => {
										manager.set_track_height(track_height);
									}
									RenderCommand::SetBackgroundEnabled(enabled) => {
										renderer.set_background_enabled(enabled);
									}
									RenderCommand::SetBackgroundOpacity(opacity) => {
										renderer.set_background_opacity(opacity);
									}
									RenderCommand::ClearAll => {
										manager.clear_danmaku();
									}
									RenderCommand::Pause => {
										manager.pause();
										paused.store(true, Ordering::SeqCst);
									}
									RenderCommand::Resume => {
										manager.resume();
										paused.store(false, Ordering::SeqCst);
									}
									RenderCommand::SetMaxDanmaku(max) => {
										manager.set_max_danmaku(max);
									}
									RenderCommand::Quit => running = false,
								}
							}
						}

						manager.update(dt);
						if let Err(e) = renderer.render(manager.active_items()) {
							log::error!("Render error: {}", e);
						}
					}
					_ => {
						let _ = TranslateMessage(&msg);
						DispatchMessageW(&msg);
					}
				}
			} else {
				std::thread::sleep(std::time::Duration::from_millis(1));
			}
		}

		KillTimer(Some(hwnd), timer_id)?;
		let _ = DestroyWindow(hwnd);
		Ok(())
	}
}

/// 创建全屏顶层覆盖层窗口。
///
/// 窗口样式为 `WS_EX_LAYERED | WS_EX_TRANSPARENT`：
/// - `WS_EX_LAYERED` — 分层窗口，支持 alpha 混合
/// - `WS_EX_TRANSPARENT` — 鼠标事件穿透到下方窗口
/// - `WS_EX_TOOLWINDOW` — 不在任务栏显示
/// - `WS_EX_NOACTIVATE` — 点击窗口不获取焦点
///
/// 创建后必须调用 `SetLayeredWindowAttributes` 设置 alpha=255，
/// 否则分层窗口默认 alpha=0 完全不可见（DComp 内容也因此不可见）。
///
/// # 参数
///
/// - `width` — 窗口宽度（物理像素，通常为屏幕宽度）
/// - `height` — 窗口高度（物理像素，通常为屏幕高度）
///
/// # 返回值
///
/// 返回创建的窗口句柄 `HWND`。
unsafe fn create_window(width: u32, height: u32) -> Result<HWND> {
	unsafe {
		let class_name = HSTRING::from("OverlayTxtWindow");
		let hinstance = GetModuleHandleW(None)?;

		// 注册窗口类（多次调用相同参数安全）
		let wc = WNDCLASSW {
			style: CS_HREDRAW | CS_VREDRAW,
			lpfnWndProc: Some(window_proc),
			cbClsExtra: 0,
			cbWndExtra: 0,
			hInstance: hinstance.into(),
			hIcon: HICON::default(),
			hCursor: HCURSOR::default(),
			hbrBackground: HBRUSH::default(),
			lpszMenuName: PCWSTR::null(),
			lpszClassName: PCWSTR::from_raw(class_name.as_ptr()),
		};
		RegisterClassW(&wc);

		let hwnd = CreateWindowExW(
			WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_LAYERED | WS_EX_TRANSPARENT,
			&class_name,
			&HSTRING::from("OverlayTxt"),
			WS_POPUP | WS_VISIBLE,
			0,
			0,
			width as i32,
			height as i32,
			None,
			None,
			Some(hinstance.into()),
			None,
		)?;

		if hwnd.0.is_null() {
			return Err(Error::new(E_FAIL, "CreateWindowExW returned null HWND"));
		}

		let _ = SetWindowPos(
			hwnd,
			Some(HWND_TOPMOST),
			0,
			0,
			width as i32,
			height as i32,
			SWP_SHOWWINDOW | SWP_NOACTIVATE,
		);

		// 分层窗口默认 alpha=0（完全不可见）
		// 设置 alpha=255 让 DComp 内容可见
		let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), 255, LWA_ALPHA);

		Ok(hwnd)
	}
}

/// 窗口过程函数。
///
/// 所有消息交由 [`DefWindowProcW`] 默认处理。
unsafe extern "system" fn window_proc(
	hwnd: HWND,
	msg: u32,
	wparam: WPARAM,
	lparam: LPARAM,
) -> LRESULT {
	unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

/// 获取虚拟桌面尺寸（物理像素）。
///
/// 需要在 DPI 感知声明后调用，否则返回的是逻辑像素值。
/// 返回值格式：`(宽度, 高度)`。
pub fn get_virtual_screen_size() -> (u32, u32) {
	unsafe {
		let width = GetSystemMetrics(SM_CXSCREEN);
		let height = GetSystemMetrics(SM_CYSCREEN);
		(width as u32, height as u32)
	}
}
