use windows::Win32::Graphics::Direct2D::ID2D1Bitmap;

/// 弹幕管理器 — 持有所有活跃弹幕，处理生命周期和轨道分配。
///
/// 职责：
/// - 接收新弹幕并分配到合适的轨道
/// - 每帧更新弹幕位置
/// - 标记超出屏幕的弹幕为死亡并释放轨道
/// - 提供活跃弹幕列表供渲染器使用
pub struct DanmakuManager {
	/// 活跃弹幕列表。
	items: Vec<DanmakuItem>,
	/// 轨道分配系统。
	track_system: TrackSystem,
	/// 屏幕宽度（像素）。
	screen_width: f32,
	/// 屏幕高度（像素）。
	screen_height: f32,
	/// 弹幕显示区域占屏幕高度的比例（0.0 ~ 1.0）。
	display_area_percent: f32,
	/// 最大并发弹幕数量，0 表示不限制。
	max_items: usize,
	/// 是否暂停弹幕动画。
	paused: bool,
}

impl DanmakuManager {
	/// 创建弹幕管理器。
	///
	/// # 参数
	///
	/// - `screen_width` — 屏幕宽度（像素），弹幕从此位置右侧进入屏幕
	/// - `screen_height` — 屏幕高度（像素），决定轨道总数
	/// - `track_height` — 每个轨道的高度（像素），30~60 为建议范围
	/// - `display_area_percent` — 弹幕显示区域占屏幕高度的比例（0.0~1.0）
	pub fn new(
		screen_width: f32,
		screen_height: f32,
		track_height: f32,
		display_area_percent: f32,
	) -> Self {
		let display_height = screen_height * display_area_percent;
		Self {
			items: Vec::new(),
			track_system: TrackSystem::new(display_height, track_height),
			screen_width,
			screen_height,
			display_area_percent,
			max_items: 0,
			paused: false,
		}
	}

	/// 添加一条纯文字弹幕（兼容旧 API）。
	///
	/// 自动分配轨道，弹幕从屏幕右边缘进入。`text` 为预编码的 UTF-16。
	pub fn add_text(
		&mut self,
		text: Vec<u16>,
		font_size: f32,
		color: [u8; 4],
		speed: f32,
		text_width: f32,
	) {
		if self.max_items > 0 && self.items.len() >= self.max_items {
			return;
		}
		let track = match self.track_system.allocate_track() {
			Some(t) => t,
			None => return,
		};
		let y = self.track_system.track_y(track);

		let segment = ProcessedSegment::Text { text, font_size, color, width: text_width };
		let total_width = text_width;

		self.items.push(DanmakuItem {
			segments: vec![segment],
			speed,
			x: self.screen_width,
			y,
			track,
			total_width,
			alive: true,
		});
	}

	/// 添加一条图文混排弹幕。
	///
	/// 段列表中的每个元素按顺序水平排列。
	///
	/// # 参数
	///
	/// - `segments` — 预处理后的段列表（Text 和 Image 交错排列）
	/// - `speed` — 水平移动速度（像素/秒）
	/// - `total_width` — 所有段的总宽度（像素），由调用方计算
	pub fn add_inline(&mut self, segments: Vec<ProcessedSegment>, speed: f32, total_width: f32) {
		if self.max_items > 0 && self.items.len() >= self.max_items {
			return;
		}
		let track = match self.track_system.allocate_track() {
			Some(t) => t,
			None => return,
		};
		let y = self.track_system.track_y(track);

		self.items.push(DanmakuItem {
			segments,
			speed,
			x: self.screen_width,
			y,
			track,
			total_width,
			alive: true,
		});
	}

	/// 更新所有弹幕位置，每帧调用一次。
	///
	/// 根据时间增量 `dt` 向左移动弹幕位置。弹幕完全移出左边缘
	/// 时标记为死亡并释放轨道。
	///
	/// # 参数
	///
	/// - `dt` — 距上一帧的时间差（秒），通常 0.016（60 FPS）
	///
	/// # 返回值
	///
	/// 如果还有任意弹幕存活返回 `true`，否则返回 `false`。
	pub fn update(&mut self, dt: f32) -> bool {
		if self.paused {
			return !self.items.is_empty();
		}

		let mut any_alive = false;

		for item in &mut self.items {
			if !item.alive {
				continue;
			}

			item.x -= item.speed * dt;

			if item.x + item.total_width < 0.0 {
				item.alive = false;
				self.track_system.release_track(item.track);
			} else {
				any_alive = true;
			}
		}

		self.items.retain(|item| item.alive);

		any_alive
	}

	/// 获取当前活跃弹幕列表的引用。
	///
	/// 返回值供渲染器使用。
	pub fn active_items(&self) -> &[DanmakuItem] {
		&self.items
	}

	/// 获取屏幕宽度（像素）。
	#[allow(dead_code)]
	pub fn screen_width(&self) -> f32 {
		self.screen_width
	}

	/// 获取屏幕高度（像素）。
	#[allow(dead_code)]
	pub fn screen_height(&self) -> f32 {
		self.screen_height
	}

	/// 更新屏幕尺寸。
	///
	/// 在显示器分辨率变化时调用，会重新初始化轨道系统。
	#[allow(dead_code)]
	pub fn resize(&mut self, width: f32, height: f32) {
		self.screen_width = width;
		self.screen_height = height;
		let display_height = height * self.display_area_percent;
		self.track_system = TrackSystem::new(display_height, self.track_system.track_height());
	}

	/// 更新弹幕显示区域比例，实时重新分配轨道。
	pub fn set_display_area_percent(&mut self, percent: f32) {
		self.display_area_percent = percent;
		let display_height = self.screen_height * percent;
		self.track_system = TrackSystem::new(display_height, self.track_system.track_height());
	}

	/// 更新弹幕轨道高度，实时重新分配轨道。
	///
	/// 不影响已显示的弹幕，新弹幕按新轨道高度排列。
	pub fn set_track_height(&mut self, track_height: f32) {
		let display_height = self.screen_height * self.display_area_percent;
		self.track_system = TrackSystem::new(display_height, track_height);
	}

	/// 清空所有活跃弹幕并释放所有轨道。
	pub fn clear_danmaku(&mut self) {
		self.track_system.release_all();
		self.items.clear();
	}

	/// 设置最大并发弹幕数量。
	///
	/// 超过数量的弹幕在添加时会被自动丢弃。0 表示不限制（默认）。
	pub fn set_max_danmaku(&mut self, max: usize) {
		self.max_items = max;
	}

	/// 暂停弹幕动画（停止移动，保持当前位置）。
	pub fn pause(&mut self) {
		self.paused = true;
	}

	/// 恢复弹幕动画。
	pub fn resume(&mut self) {
		self.paused = false;
	}
}

/// 单个弹幕项。
///
/// 包含一个或多个内容段（文字/图片），共享相同的速度。
#[derive(Clone)]
pub struct DanmakuItem {
	/// 内容段列表
	pub segments: Vec<ProcessedSegment>,
	/// 水平移动速度（像素/秒）
	pub speed: f32,
	/// 当前 X 坐标（右边缘起始）
	pub x: f32,
	/// 当前 Y 坐标（轨道顶部）
	pub y: f32,
	/// 分配的轨道索引
	pub track: usize,
	/// 所有段的总宽度
	pub total_width: f32,
	/// 是否存活
	pub alive: bool,
}

/// 预处理后的弹幕内容段。
///
/// 由渲染线程在添加弹幕时构建，Text 段包含预编码的 UTF-16 文本
/// 和已测量的宽度，Image 段包含加载完成的 D2D bitmap。
#[derive(Clone)]
pub enum ProcessedSegment {
	/// 文字段
	Text {
		/// 文本内容（预编码为 UTF-16，避免每帧临时分配）
		text: Vec<u16>,
		/// 字体大小（像素）
		font_size: f32,
		/// RGBA 颜色
		color: [u8; 4],
		/// 文本渲染宽度（像素），由外部预先测量
		width: f32,
	},
	/// 图片段
	Image {
		/// D2D bitmap（render-thread only，COM 对象）
		bitmap: ID2D1Bitmap,
		/// 渲染宽度（像素）
		width: f32,
		/// 渲染高度（像素）
		height: f32,
		/// 垂直偏移量（像素），用于对齐文字 baseline
		y_offset: f32,
	},
}

/// 用户侧的内嵌内容段。
///
/// 传递给 [`send_inline`](crate::OverlayTxt::send_inline) 以描述
/// 一条弹幕中的混合内容。Image 段使用原始 RGBA 字节数据，
/// 在渲染线程加载为 D2D bitmap。
///
/// # 示例
///
/// ```no_run
/// use overlaytxt::InlineContent;
///
/// let content = vec![
///     InlineContent::text("Hello ", 28.0, [255; 4]),
///     InlineContent::rgba_image(32, 32, &[/* RGBA 数据 */]),
///     InlineContent::text(" World", 28.0, [255; 4]),
/// ];
/// ```
#[derive(Clone)]
pub enum InlineContent {
	/// 纯文字段
	Text {
		/// 弹幕文字
		text: String,
		/// 字体大小（像素）
		font_size: f32,
		/// RGBA 颜色
		color: [u8; 4],
	},
	/// 图片段（原始 RGBA 像素数据）
	Image {
		/// RGBA 像素数据（ straight alpha 会自动转为 pre-multiplied alpha 在创建 bitmap 之前）
		rgba: Vec<u8>,
		/// 图片宽度（像素）
		width: u32,
		/// 图片高度（像素）
		height: u32,
	},
}

impl InlineContent {
	/// 创建纯文字段。
	pub fn text(text: &str, font_size: f32, color: [u8; 4]) -> Self {
		Self::Text { text: text.to_string(), font_size, color }
	}

	/// 创建图片段。
	///
	/// 输入数据为 **straight alpha**（普通 RGBA），会在创建 bitmap 前自动转换为
	/// **pre-multiplied alpha**，调用者无需手动转换。
	pub fn rgba_image(width: u32, height: u32, rgba: &[u8]) -> Self {
		if rgba.chunks_exact(4).len() != (width * height) as usize {
			// 用户提供了不完整的数据，返回全零（但不会panic，renderer会跳过创建）
		}
		Self::Image { rgba: straight_to_premul(rgba), width, height }
	}
}

/// 轨道系统 — 管理弹幕轨道分配，避免水平方向弹幕重叠。
///
/// 将屏幕垂直方向划分为等高的轨道，每条弹幕占用一个轨道，
/// 通过统计每个轨道当前的弹幕数量来实现负载均衡。
pub struct TrackSystem {
	/// 每个轨道的高度（像素）。
	track_height: f32,
	/// 总轨道数。
	#[allow(dead_code)]
	num_tracks: usize,
	/// 每个轨道的当前弹幕计数，用于负载均衡。
	track_counts: Vec<u32>,
}

impl TrackSystem {
	/// 创建轨道系统。
	///
	/// 轨道数量 = `max(1, screen_height / track_height)`。
	pub fn new(screen_height: f32, track_height: f32) -> Self {
		let num_tracks = ((screen_height / track_height) as usize).max(1);
		Self { track_height, num_tracks, track_counts: vec![0; num_tracks] }
	}

	/// 分配一个负载最轻的轨道。
	pub fn allocate_track(&mut self) -> Option<usize> {
		let (min_idx, _) = self.track_counts.iter().enumerate().min_by_key(|&(_, count)| count)?;
		self.track_counts[min_idx] += 1;
		Some(min_idx)
	}

	/// 释放轨道。
	pub fn release_track(&mut self, track: usize) {
		if track < self.track_counts.len() {
			self.track_counts[track] = self.track_counts[track].saturating_sub(1);
		}
	}

	/// 释放所有轨道（清空计数）。
	pub fn release_all(&mut self) {
		for c in &mut self.track_counts {
			*c = 0;
		}
	}

	/// 计算轨道在屏幕上的 Y 坐标。
	pub fn track_y(&self, track: usize) -> f32 {
		track as f32 * self.track_height
	}

	pub fn track_height(&self) -> f32 {
		self.track_height
	}

	#[allow(dead_code)]
	pub fn num_tracks(&self) -> usize {
		self.num_tracks
	}
}

/// 将 straight alpha RGBA 像素数据转换为预乘 alpha。
///
/// D2D 的 `DXGI_ALPHA_MODE_PREMULTIPLIED` 要求像素的 RGB 值已经乘以 alpha，
/// 而大多数图片格式（PNG、JPEG）使用 straight alpha（非预乘）。
///
/// 转换公式：`premul_C = C * A / 255`
///
/// # 参数
///
/// - `rgba` — straight alpha RGBA 像素数据，每像素 4 字节
///
/// # 返回值
///
/// 返回预乘后的 RGBA 像素数据（长度与输入相同）。
///
/// # 示例
///
/// ```
/// use overlaytxt::straight_to_premul;
/// let straight = [128, 64, 32, 128];  // 半透明
/// let premul = straight_to_premul(&straight);
/// assert_eq!(premul, [64, 32, 16, 128]);  // R=128*128/255=64, G=64*128/255=32, ...
/// ```
pub fn straight_to_premul(rgba: &[u8]) -> Vec<u8> {
	rgba.chunks_exact(4)
		.flat_map(|p| {
			let r = p[0] as u32;
			let g = p[1] as u32;
			let b = p[2] as u32;
			let a = p[3] as u32;
			[(r * a / 255) as u8, (g * a / 255) as u8, (b * a / 255) as u8, a as u8]
		})
		.collect()
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_track_system_allocate_release() {
		let mut ts = TrackSystem::new(400.0, 40.0);
		assert_eq!(ts.num_tracks(), 10);
		let t = ts.allocate_track();
		assert!(t.is_some());
		assert_eq!(ts.track_counts[t.unwrap()], 1);
		ts.release_track(t.unwrap());
		assert_eq!(ts.track_counts[t.unwrap()], 0);
	}

	#[test]
	fn test_track_y() {
		let ts = TrackSystem::new(400.0, 40.0);
		assert_eq!(ts.track_y(0), 0.0);
		assert_eq!(ts.track_y(1), 40.0);
		assert_eq!(ts.track_y(5), 200.0);
	}

	#[test]
	fn test_release_all() {
		let mut ts = TrackSystem::new(400.0, 40.0);
		let t1 = ts.allocate_track().unwrap();
		let t2 = ts.allocate_track().unwrap();
		assert_eq!(ts.track_counts[t1], 1);
		assert_eq!(ts.track_counts[t2], 1);
		ts.release_all();
		assert!(ts.track_counts.iter().all(|&c| c == 0));
	}

	#[test]
	fn test_track_system_exhausted() {
		let mut ts = TrackSystem::new(40.0, 40.0);
		assert_eq!(ts.num_tracks(), 1);
		let t = ts.allocate_track();
		assert!(t.is_some());
		// 单个轨道始终可以分配（不限制轨道数量）
		let t2 = ts.allocate_track();
		assert!(t2.is_some());
	}

	#[test]
	fn test_danmaku_manager_max_items() {
		let mut dm = DanmakuManager::new(1920.0, 1080.0, 40.0, 1.0);
		dm.set_max_danmaku(2);
		dm.add_text(vec![65; 10], 28.0, [255; 4], 150.0, 100.0);
		dm.add_text(vec![66; 10], 28.0, [255; 4], 150.0, 100.0);
		dm.add_text(vec![67; 10], 28.0, [255; 4], 150.0, 100.0);
		assert_eq!(dm.items.len(), 2);
	}

	#[test]
	fn test_pause_resume() {
		let mut dm = DanmakuManager::new(1920.0, 1080.0, 40.0, 1.0);
		dm.add_text(vec![65; 10], 28.0, [255; 4], 150.0, 100.0);
		let x_before = dm.items[0].x;
		dm.pause();
		dm.update(1.0);
		assert_eq!(dm.items[0].x, x_before);
		dm.resume();
		dm.update(1.0);
		assert!(dm.items[0].x < x_before);
	}

	#[test]
	fn test_straight_to_premul() {
		let straight = [128, 64, 32, 128];
		let premul = straight_to_premul(&straight);
		assert_eq!(premul, [64, 32, 16, 128]);
	}
}
