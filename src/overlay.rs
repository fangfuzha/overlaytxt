use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

use crate::danmaku::InlineContent;
use crate::window::*;

/// 弹幕应用错误类型。
#[derive(Debug)]
pub enum OverlayTxtError {
	/// 渲染线程创建失败。
	ThreadSpawn(String),
	/// 渲染线程 panic 退出（调用 [`wait`](OverlayTxt::wait) 时报告）。
	ThreadPanic,
	/// 渲染管线初始化失败（窗口创建、D3D/D2D/DComp 初始化等）。
	///
	/// 字符串包含具体失败原因，通常来自 DirectX 或 Win32 API 错误。
	InitFailed(String),
	/// 渲染线程已退出，命令通道已关闭。
	RendererNotRunning,
}

impl std::fmt::Display for OverlayTxtError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			Self::ThreadSpawn(msg) => write!(f, "failed to spawn render thread: {}", msg),
			Self::ThreadPanic => write!(f, "render thread panicked"),
			Self::InitFailed(msg) => write!(f, "render pipeline init failed: {}", msg),
			Self::RendererNotRunning => write!(f, "renderer is not running"),
		}
	}
}

impl std::error::Error for OverlayTxtError {}

/// 弹幕应用配置。
///
/// 传递给 [`OverlayTxt::new`] 以指定渲染参数和默认样式。
///
/// # 默认值
///
/// | 字段 | 默认值 | 说明 |
/// |------|--------|------|
/// | `track_height` | 40.0 | 每个弹幕轨道的高度（像素） |
/// | `default_font_size` | 28.0 | 弹幕默认字体大小（像素） |
/// | `default_speed` | 150.0 | 弹幕默认水平移动速度（像素/秒） |
/// | `default_color` | `[255, 255, 255, 255]` | 弹幕默认颜色（RGBA，白色） |
/// | `display_area_percent` | 1.0 | 弹幕显示区域占屏幕高度的比例 |
/// | `background_enabled` | `false` | 是否在弹幕显示区域绘制半透明背景 |
/// | `background_opacity` | 0.3 | 背景透明度（0.0~1.0），仅 `background_enabled=true` 时生效 |
#[derive(Clone)]
pub struct OverlayTxtConfig {
	/// 每个弹幕轨道的高度，单位：像素。
	///
	/// 轨道高度决定弹幕行的间距。建议值范围 30 - 60。
	/// 值越小可容纳的并行弹幕越多，但弹幕行间距越小。
	pub track_height: f32,
	/// 弹幕默认字体大小，单位：像素。
	///
	/// 使用 [`send_text`](OverlayTxt::send_text) 发送时使用此值。
	pub default_font_size: f32,
	/// 弹幕默认水平移动速度，单位：像素/秒。
	///
	/// 使用 [`send_text`](OverlayTxt::send_text) 发送时使用此值。
	/// 典型值范围 100 - 300。
	pub default_speed: f32,
	/// 弹幕默认文字颜色，`[R, G, B, A]`（0-255）。
	///
	/// 使用 [`send_text`](OverlayTxt::send_text) 发送时使用此值。
	/// 例如白色 `[255, 255, 255, 255]`，红色 `[255, 0, 0, 255]`。
	pub default_color: [u8; 4],
	/// 弹幕显示区域占屏幕高度的比例（0.0 ~ 1.0）。
	///
	/// 例如 `0.5` 表示弹幕仅在上半屏显示，下半屏全透明。
	/// 默认 `1.0`（全屏显示）。
	pub display_area_percent: f32,
	/// 是否在弹幕显示区域绘制半透明背景。
	///
	/// 开启后弹幕区有半透明暗色背景，提高文字可读性。
	/// 默认关闭，需配合 `display_area_percent < 1.0` 使用效果更佳。
	pub background_enabled: bool,
	/// 背景透明度（0.0 ~ 1.0）。
	///
	/// 仅 `background_enabled = true` 时生效。
	/// 0.0 为全透明，1.0 为完全不透明。默认 0.3。
	pub background_opacity: f32,
}

impl Default for OverlayTxtConfig {
	fn default() -> Self {
		Self {
			track_height: 40.0,
			default_font_size: 28.0,
			default_speed: 150.0,
			default_color: [255, 255, 255, 255],
			display_area_percent: 1.0,
			background_enabled: false,
			background_opacity: 0.3,
		}
	}
}

/// 弹幕应用主结构体。
///
/// 线程安全，创建后可在**任意线程**（主线程、工作线程、UI 事件回调等）
/// 通过 `send_*` 方法推送弹幕。
///
/// # 生命周期
///
/// 1. [`new`](OverlayTxt::new) — 创建实例
/// 2. [`start`](OverlayTxt::start) — 启动渲染线程和覆盖窗口（非阻塞）
/// 3. [`send_text`](OverlayTxt::send_text) / [`send_text_custom`](OverlayTxt::send_text_custom) — 实时推送弹幕
/// 4. [`wait`](OverlayTxt::wait) — 阻塞等待渲染线程结束
/// 5. [`stop`](OverlayTxt::stop) / [`Drop`] — 停止渲染并销毁窗口
///
/// # 示例
///
/// ```no_run
/// use overlaytxt::{OverlayTxt, OverlayTxtConfig};
///
/// let config = OverlayTxtConfig::default();
/// let mut app = OverlayTxt::new(config).unwrap();
/// app.start().unwrap();
///
/// // 主线程推送
/// app.send_text("Hello World");
///
/// // 其他线程推送（OverlayTxt 是 Sync）
/// std::thread::spawn(move || {
///     app.send_text_custom("来自线程", None, None, None);
/// });
/// ```
pub struct OverlayTxt {
	/// 用户配置（默认样式、轨道高度、显示区域等）。
	config: OverlayTxtConfig,
	/// 发送端 — 向渲染线程发送命令。
	command_tx: Option<mpsc::Sender<RenderCommand>>,
	/// 接收端 — 渲染线程消费命令。
	command_rx: Arc<Mutex<mpsc::Receiver<RenderCommand>>>,
	/// 渲染线程句柄。
	thread_handle: Option<JoinHandle<()>>,
	/// 是否已启动（多次调用 `start` 安全）。
	started: bool,
	/// 渲染线程是否仍在运行。
	running: Arc<AtomicBool>,
	/// 渲染线程初始化结果，用于 `wait()` 时向主线程传播错误。
	init_result: Arc<Mutex<Option<Result<(), OverlayTxtError>>>>,
	/// 弹幕动画是否暂停。
	paused: Arc<AtomicBool>,
}

// SAFETY: OverlayTxt is Send + Sync because:
// - `config` is accessed only through `&self` getters, all fields are `Copy` types
// - `command_tx` is `mpsc::Sender` which is `Send + Sync`
// - `command_rx` / `init_result` / `running` / `paused` are wrapped in `Arc<Mutex<…>>`
//   or `Arc<AtomicBool>`, both `Send + Sync`
// - `thread_handle` is `Option<JoinHandle<()>>` (`Send` but not `Sync`); it is only
//   accessed through `&mut self` methods (`start`, `wait`, `stop`, `drop`), so no
//   data race can occur through shared references.
unsafe impl Send for OverlayTxt {}
unsafe impl Sync for OverlayTxt {}

impl OverlayTxt {
	/// 创建弹幕应用实例。
	///
	/// 创建后窗口尚未显示，需调用 [`start`](Self::start) 启动。
	///
	/// # 参数
	///
	/// - `config` — 渲染参数和默认样式配置
	pub fn new(config: OverlayTxtConfig) -> Result<Self, OverlayTxtError> {
		let (tx, rx) = mpsc::channel();

		Ok(Self {
			config,
			command_tx: Some(tx),
			command_rx: Arc::new(Mutex::new(rx)),
			thread_handle: None,
			started: false,
			running: Arc::new(AtomicBool::new(true)),
			init_result: Arc::new(Mutex::new(None)),
			paused: Arc::new(AtomicBool::new(false)),
		})
	}

	/// 启动弹幕窗口和渲染循环（非阻塞）。
	///
	/// 启动后覆盖窗口立即显示在屏幕最顶层，调用方可在任意线程
	/// 通过 `send_*` 方法实时推送弹幕。
	///
	/// 内部执行以下操作：
	/// 1. 声明当前进程为 DPI 感知
	/// 2. 创建全屏顶层透明窗口（`WS_EX_LAYERED / WS_EX_TRANSPARENT`）
	/// 3. 初始化 D3D11 + D2D + DWrite + DComp 渲染管线
	/// 4. 启动基于 `WM_TIMER` 的 60 FPS 渲染循环
	///
	/// 多次调用安全，第二次及后续调用直接返回 `Ok`。
	pub fn start(&mut self) -> Result<(), OverlayTxtError> {
		if self.started {
			return Ok(());
		}

		// 声明进程为 DPI 感知，避免 DPI 虚拟化导致窗口尺寸（逻辑像素）与
		// swapchain 尺寸（物理像素）不匹配，使 DComp 内容显示异常。
		// 必须在创建窗口前调用。
		set_process_dpi_aware();

		let (width, height) = get_virtual_screen_size();
		let config = self.config.clone();
		let command_rx = self.command_rx.clone();
		let running = self.running.clone();
		let init_result = self.init_result.clone();
		let paused = self.paused.clone();

		let handle = thread::Builder::new()
			.name("overlaytxt-render".into())
			.spawn(move || {
				let window_config = WindowConfig {
					screen_width: width,
					screen_height: height,
					track_height: config.track_height,
					display_area_percent: config.display_area_percent,
					background_enabled: config.background_enabled,
					background_opacity: config.background_opacity,
				};

				let result = run_window(window_config, command_rx, paused);
				match &result {
					Ok(()) => {
						*init_result.lock().unwrap() = Some(Ok(()));
					}
					Err(e) => {
						log::error!("OverlayTxt error: {}", e);
						*init_result.lock().unwrap() =
							Some(Err(OverlayTxtError::InitFailed(e.to_string())));
					}
				}
				running.store(false, Ordering::SeqCst);
			})
			.map_err(|e| OverlayTxtError::ThreadSpawn(e.to_string()))?;

		self.thread_handle = Some(handle);
		self.started = true;

		Ok(())
	}

	/// 发送一条弹幕，使用配置中的默认样式。
	///
	/// # 参数
	///
	/// - `text` — 弹幕文字内容
	///
	/// 样式使用 [`OverlayTxtConfig`] 中的 `default_font_size`、
	/// `default_color`、`default_speed`。
	pub fn send_text(&self, text: &str) {
		self.send_text_custom(text, None, None, None);
	}

	/// 发送一条图文混排弹幕。
	///
	/// # 参数
	///
	/// - `segments` — 内容段列表，Text（纯文字）和 Image（图片）交错排列
	/// - `speed` — 水平移动速度，单位：像素/秒。`None` 表示使用默认速度
	///
	/// # 示例
	///
	/// ```no_run
	/// # use overlaytxt::{OverlayTxt, OverlayTxtConfig, InlineContent};
	/// # let mut app = OverlayTxt::new(OverlayTxtConfig::default()).unwrap();
	/// # app.start().unwrap();
	/// app.send_inline(vec![
	///     InlineContent::text("Hello 😀 ", 28.0, [255; 4]),
	///     InlineContent::rgba_image(32, 32, &[/* RGBA 像素数据（straight alpha） */]),
	///     InlineContent::text(" 图片示例", 28.0, [255; 4]),
	/// ], None);
	/// ```
	///
	/// # 线程安全
	///
	/// 内部通过 `mpsc::Sender` 发送命令到渲染线程，不阻塞发送方。
	pub fn send_inline(&self, segments: Vec<InlineContent>, speed: Option<f32>) {
		let spd = speed.unwrap_or(self.config.default_speed);
		if let Some(tx) = &self.command_tx {
			let _ = tx.send(RenderCommand::AddInlineDanmaku { segments, speed: spd });
		}
	}

	/// 发送一条弹幕，自定义样式参数。
	///
	/// 后三个参数可传 `None`，表示使用 [`OverlayTxtConfig`] 中的默认值。
	///
	/// 此方法线程安全，可在任意线程调用。
	///
	/// # 参数
	///
	/// - `text` — 弹幕文字内容
	/// - `font_size` — 字体大小，单位：像素。建议 16 - 72。`None` 表示默认值
	/// - `color` — RGBA 颜色数组，每个通道 0-255。`None` 表示默认颜色
	/// - `speed` — 水平移动速度，单位：像素/秒。典型值 100 - 300。`None` 表示默认值
	///
	/// # 示例
	///
	/// ```no_run
	/// # use overlaytxt::{OverlayTxt, OverlayTxtConfig};
	/// # let mut app = OverlayTxt::new(OverlayTxtConfig::default()).unwrap();
	/// # app.start().unwrap();
	/// // 全部使用默认值
	/// app.send_text_custom("Hello", None, None, None);
	///
	/// // 只自定义颜色
	/// app.send_text_custom("Red", None, Some([255, 0, 0, 255]), None);
	///
	/// // 全自定义
	/// app.send_text_custom("Big", Some(48.0), Some([255, 255, 0, 255]), Some(100.0));
	/// ```
	///
	/// # 线程安全
	///
	/// 内部通过 `mpsc::Sender` 发送命令到渲染线程，不阻塞发送方。
	/// 如果渲染线程已退出（窗口已销毁），命令被静默丢弃。
	pub fn send_text_custom(
		&self,
		text: &str,
		font_size: Option<f32>,
		color: Option<[u8; 4]>,
		speed: Option<f32>,
	) {
		let font_size = font_size.unwrap_or(self.config.default_font_size);
		let color = color.unwrap_or(self.config.default_color);
		let speed = speed.unwrap_or(self.config.default_speed);
		if let Some(tx) = &self.command_tx {
			let cmd = RenderCommand::AddDanmaku { text: text.to_string(), font_size, color, speed };
			let _ = tx.send(cmd);
		}
	}

	/// 阻塞等待渲染线程结束。
	///
	/// 窗口被关闭后（按 Alt+Esc 或调用 [`stop`](Self::stop)），
	/// 此方法等待渲染线程完全退出后返回。
	///
	/// 如果渲染管线初始化失败或渲染线程 panic，返回对应的错误。
	pub fn wait(&mut self) -> Result<(), OverlayTxtError> {
		if let Some(handle) = self.thread_handle.take() {
			handle.join().map_err(|_| OverlayTxtError::ThreadPanic)?;
		}
		// 检查渲染线程的初始化结果
		let result = self.init_result.lock().unwrap().take();
		if let Some(Err(e)) = result {
			return Err(e);
		}
		Ok(())
	}

	/// 发送 Quit 命令通知渲染线程退出。
	///
	/// 调用后窗口立即收到退出信号，下一个消息循环迭代中退出。
	/// 也会消耗内部的命令发送器，后续 `send_*` 调用静默丢弃。
	///
	/// 此方法在 [`Drop`] 实现中自动调用，通常不需要手动调用。
	pub fn stop(&mut self) {
		if let Some(tx) = self.command_tx.take() {
			let _ = tx.send(RenderCommand::Quit);
		}
	}

	/// 检查渲染线程是否仍在运行。
	///
	/// 窗口正常运行时返回 `true`，渲染线程退出后返回 `false`。
	/// 可用于业务循环的退出条件判断。
	pub fn is_running(&self) -> bool {
		self.running.load(Ordering::SeqCst)
	}

	/// 暂停弹幕动画。
	///
	/// 暂停后所有弹幕停止移动，保留在当前位置。已在屏幕上的弹幕保持可见。
	/// 调用 [`resume`](Self::resume) 恢复滚动。
	pub fn pause(&mut self) {
		if let Some(tx) = &self.command_tx {
			let _ = tx.send(RenderCommand::Pause);
		}
	}

	/// 恢复弹幕动画。
	///
	/// 从暂停位置继续滚动弹幕。
	pub fn resume(&mut self) {
		if let Some(tx) = &self.command_tx {
			let _ = tx.send(RenderCommand::Resume);
		}
	}

	/// 检查弹幕动画是否处于暂停状态。
	pub fn is_paused(&self) -> bool {
		self.paused.load(Ordering::SeqCst)
	}

	// ── 默认值设置 ──

	/// 设置弹幕默认字体大小。
	///
	/// 影响后续 `send_text`、`send_text_custom` 调用中未指定字体大小时的默认值。
	pub fn set_default_font_size(&mut self, font_size: f32) {
		self.config.default_font_size = font_size;
	}

	/// 设置弹幕默认水平移动速度。
	///
	/// 影响后续 `send_text`、`send_text_custom`、`send_inline` 调用中
	/// 未指定速度时的默认值。
	pub fn set_default_speed(&mut self, speed: f32) {
		self.config.default_speed = speed;
	}

	/// 设置弹幕默认颜色。
	///
	/// 影响后续 `send_text`、`send_text_custom` 调用中未指定颜色时的默认值。
	pub fn set_default_color(&mut self, color: [u8; 4]) {
		self.config.default_color = color;
	}

	/// 设置弹幕轨道高度，实时重新分配轨道。
	///
	/// 轨道高度决定弹幕行的间距。建议值范围 30 - 60。
	/// 值越小在同一显示区域内可容纳的并行弹幕越多。
	/// 不影响已显示的弹幕，新弹幕按新轨道高度排列。
	pub fn set_track_height(&mut self, track_height: f32) {
		self.config.track_height = track_height;
		if let Some(tx) = &self.command_tx {
			let _ = tx.send(RenderCommand::SetTrackHeight(track_height));
		}
	}

	/// 设置弹幕显示区域占屏幕高度的比例。
	///
	/// 实时生效，渲染线程会重新分配轨道并在新的区域绘制背景。
	/// 范围 0.01 ~ 1.0，超出范围会自动 clamp。
	///
	/// # 参数
	///
	/// - `percent` — 显示区域比例。`0.5` 表示仅上半屏显示，`1.0` 表示全屏
	pub fn set_display_area_percent(&mut self, percent: f32) {
		let percent = percent.clamp(0.01, 1.0);
		self.config.display_area_percent = percent;
		if let Some(tx) = &self.command_tx {
			let _ = tx.send(RenderCommand::SetDisplayAreaPercent(percent));
		}
	}

	/// 设置是否在弹幕显示区域绘制半透明背景。
	///
	/// 实时生效，开启后弹幕区显示半透明暗色条，提高文字可读性。
	/// 与 `display_area_percent` 配合使用效果最佳。
	pub fn set_background_enabled(&mut self, enabled: bool) {
		self.config.background_enabled = enabled;
		if let Some(tx) = &self.command_tx {
			let _ = tx.send(RenderCommand::SetBackgroundEnabled(enabled));
		}
	}

	/// 设置弹幕显示区域的背景透明度。
	///
	/// 实时生效，仅 `background_enabled = true` 时有意义。
	/// 范围 0.0（全透明）~ 1.0（不透明），建议 0.2 ~ 0.5。
	pub fn set_background_opacity(&mut self, opacity: f32) {
		let opacity = opacity.clamp(0.0, 1.0);
		self.config.background_opacity = opacity;
		if let Some(tx) = &self.command_tx {
			let _ = tx.send(RenderCommand::SetBackgroundOpacity(opacity));
		}
	}

	/// 清空当前屏幕上所有弹幕。
	///
	/// 立即移除所有活跃弹幕并释放轨道，不影响后续新弹幕的添加。
	pub fn clear_all(&mut self) {
		if let Some(tx) = &self.command_tx {
			let _ = tx.send(RenderCommand::ClearAll);
		}
	}

	/// 设置最大并发弹幕数量。
	///
	/// 超过该数量的弹幕在进入时会被自动丢弃。设为 0 表示不限制（默认）。
	/// 不影响已在屏幕上的弹幕。
	pub fn set_max_danmaku(&mut self, max: usize) {
		if let Some(tx) = &self.command_tx {
			let _ = tx.send(RenderCommand::SetMaxDanmaku(max));
		}
	}

	// ── 默认值获取 ──

	/// 获取当前弹幕默认字体大小。
	pub fn default_font_size(&self) -> f32 {
		self.config.default_font_size
	}

	/// 获取当前弹幕默认水平移动速度。
	pub fn default_speed(&self) -> f32 {
		self.config.default_speed
	}

	/// 获取当前弹幕默认颜色。
	pub fn default_color(&self) -> [u8; 4] {
		self.config.default_color
	}

	/// 获取当前弹幕轨道高度。
	pub fn track_height(&self) -> f32 {
		self.config.track_height
	}

	/// 获取当前弹幕显示区域占屏幕高度的比例。
	pub fn display_area_percent(&self) -> f32 {
		self.config.display_area_percent
	}

	/// 获取当前背景是否开启。
	pub fn background_enabled(&self) -> bool {
		self.config.background_enabled
	}

	/// 获取当前背景透明度。
	pub fn background_opacity(&self) -> f32 {
		self.config.background_opacity
	}
}

impl Drop for OverlayTxt {
	fn drop(&mut self) {
		self.stop();
	}
}

/// 声明当前进程为 DPI 感知。
///
/// 未声明 DPI 感知时，Windows 会对进程进行 DPI 虚拟化：
/// - `GetSystemMetrics(SM_CXSCREEN)` 返回逻辑像素（如 1440）
/// - 但 DWM 会把窗口拉伸到物理尺寸（如 2880）
/// - swapchain 用逻辑像素尺寸创建，与窗口物理尺寸不匹配
/// - DComp 内容会显示异常或不可见
///
/// 声明 DPI 感知后，`GetSystemMetrics` 返回物理像素，窗口与 swapchain 尺寸一致。
///
/// # 注意
///
/// 必须在创建任何窗口之前调用，否则不生效。
fn set_process_dpi_aware() {
	#[link(name = "user32")]
	unsafe extern "system" {
		fn SetProcessDPIAware() -> i32;
	}
	unsafe {
		let _ = SetProcessDPIAware();
	}
}
