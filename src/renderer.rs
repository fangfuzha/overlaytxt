use std::cell::RefCell;
use std::collections::HashMap;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Direct2D::Common::{
	D2D_RECT_F, D2D_SIZE_U, D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_COLOR_F, D2D1_PIXEL_FORMAT,
};
use windows::Win32::Graphics::Direct2D::*;
use windows::Win32::Graphics::Direct3D::*;
use windows::Win32::Graphics::Direct3D11::*;
use windows::Win32::Graphics::DirectComposition::*;
use windows::Win32::Graphics::DirectWrite::*;
use windows::Win32::Graphics::Dxgi::Common::*;
use windows::Win32::Graphics::Dxgi::*;
use windows::core::*;
use windows_numerics::Vector2;

use crate::danmaku::{DanmakuItem, ProcessedSegment};

/// DirectComposition + Direct2D + DirectWrite 渲染器。
///
/// 管理完整的 D3D11 → D2D → DWrite → DXGI swapchain → DComp 渲染管线。
///
/// # 渲染管线
///
/// ```text
/// D3D11CreateDevice → IDXGIDevice → ID2D1Device → ID2D1DeviceContext
///                                → IDXGISwapChain1 (CreateSwapChainForComposition)
///                                → IDCompositionDevice → IDCompositionVisual → IDCompositionTarget
///
/// 每帧：GetBuffer → CreateBitmapFromDxgiSurface → BeginDraw → DrawText → EndDraw → Present → Commit
/// ```
///
/// # 关键注意点
///
/// - `IDCompositionTarget` 必须保存在结构体中，否则 Drop 后 DComp 与窗口的绑定解除
/// - D2D bitmap 的 `alphaMode` 必须与 swapchain 一致（都用 `PREMULTIPLIED`），
///   否则 Clear(alpha=0) 输出不透明黑色
pub struct DcompRenderer {
	/// DComp 设备对象，用于 Commit 和 Visual 操作。
	dcomp_device: IDCompositionDevice,
	/// DComp 视觉对象，持有 swapchain 内容。
	#[allow(dead_code)]
	dcomp_visual: IDCompositionVisual,
	/// DComp 与窗口的绑定句柄。
	///
	/// 必须保存，否则 Drop 后绑定解除，Commit 失效。
	/// 与其他 COM 对象不同，此对象销毁意味着整层绑定消失。
	#[allow(dead_code)]
	dcomp_target: IDCompositionTarget,
	/// D2D 设备上下文，用于绘制弹幕。
	d2d_context: ID2D1DeviceContext,
	/// DWrite 工厂，用于创建 TextFormat 和 TextLayout。
	dwrite_factory: IDWriteFactory,
	/// 自定义 font fallback（彩色 emoji 支持）。
	///
	/// `None` 表示系统不支持（非 Windows 10+）。
	emoji_fallback: Option<IDWriteFontFallback>,
	/// DXGI 交换链，通过 DComp 合成到窗口。
	///
	/// 使用 `IDXGISwapChain3` 以访问 `GetCurrentBackBufferIndex`（DXGI 1.3+，Windows 8.1+）。
	swapchain: IDXGISwapChain3,
	/// 屏幕宽度（物理像素）。
	screen_width: u32,
	/// 屏幕高度（物理像素）。
	#[allow(dead_code)]
	screen_height: u32,
	/// 弹幕显示区域高度（像素），由 `set_display_area_percent` 驱动更新。
	display_area_height: f32,
	/// 是否在弹幕显示区域绘制半透明背景。
	#[allow(dead_code)]
	background_enabled: bool,
	/// 背景透明度（0.0 ~ 1.0）。
	background_opacity: f32,
	/// DWrite TextFormat 缓存，key = `font_size as u32`。
	text_format_cache: RefCell<HashMap<u32, IDWriteTextFormat>>,
	/// D2D SolidColorBrush 缓存，key = RGBA 编码为 u64。
	solid_brush_cache: RefCell<HashMap<u64, ID2D1SolidColorBrush>>,
	/// swapchain 后缓冲区对应的 D2D bitmap（按缓冲区索引缓存，避免每帧重建）。
	/// FLIP_SEQUENTIAL + BufferCount=2，所以 2 个条目。
	back_buffer_bitmaps: [Option<ID2D1Bitmap1>; 2],
	/// 背景画刷缓存（仅 `background_enabled` 时使用），opacity 变化时通过 SetColor 更新。
	bg_brush: Option<ID2D1SolidColorBrush>,
}

impl DcompRenderer {
	/// 初始化完整的渲染管线。
	///
	/// 按顺序创建以下对象：
	/// 1. D3D11 设备（`D3D11_CREATE_DEVICE_BGRA_SUPPORT`）
	/// 2. DXGI 设备
	/// 3. D2D 设备 + DeviceContext
	/// 4. DXGI swapchain（`CreateSwapChainForComposition`，`DXGI_ALPHA_MODE_PREMULTIPLIED`）
	/// 5. DComp 设备 + Visual + Target（绑定到 `hwnd`）
	/// 6. DWrite 工厂 + 自定义 font fallback（彩色 emoji 支持）
	///
	/// # 参数
	///
	/// - `hwnd` — 目标窗口句柄，DComp 将渲染输出绑定到此窗口
	/// - `width` — swapchain 宽度（物理像素），应与窗口客户区一致
	/// - `height` — swapchain 高度（物理像素）
	/// - `background_enabled` — 是否在弹幕显示区域绘制半透明背景
	/// - `background_opacity` — 背景透明度（0.0 ~ 1.0）
	pub fn new(
		hwnd: HWND,
		width: u32,
		height: u32,
		background_enabled: bool,
		background_opacity: f32,
	) -> Result<Self> {
		unsafe {
			// ── 1. D3D11 设备 ──
			let mut d3d: Option<ID3D11Device> = None;
			let mut _ctx: Option<ID3D11DeviceContext> = None;
			D3D11CreateDevice(
				None,
				D3D_DRIVER_TYPE_HARDWARE,
				HMODULE::default(),
				D3D11_CREATE_DEVICE_BGRA_SUPPORT,
				None,
				D3D11_SDK_VERSION,
				Some(&mut d3d),
				None,
				Some(&mut _ctx),
			)?;
			let d3d = d3d.ok_or_else(|| Error::new(E_FAIL, "D3D11 device failed"))?;

			// ── 2. DXGI 设备 ──
			let dxgi: IDXGIDevice = d3d.cast()?;

			// ── 3. D2D ──
			let d2d_factory: ID2D1Factory1 =
				D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)?;
			let d2d_dev: ID2D1Device = d2d_factory.CreateDevice(&dxgi)?;
			let d2d_ctx: ID2D1DeviceContext =
				d2d_dev.CreateDeviceContext(D2D1_DEVICE_CONTEXT_OPTIONS_NONE)?;

			// ── 4. DXGI 交换链（用于 DComp） ──
			let adapter: IDXGIAdapter = dxgi.GetAdapter()?;
			let factory: IDXGIFactory2 = adapter.GetParent()?;

			let desc = DXGI_SWAP_CHAIN_DESC1 {
				Width: width,
				Height: height,
				Format: DXGI_FORMAT_B8G8R8A8_UNORM,
				Stereo: false.into(),
				SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
				BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
				BufferCount: 2,
				Scaling: DXGI_SCALING_STRETCH,
				SwapEffect: DXGI_SWAP_EFFECT_FLIP_SEQUENTIAL,
				AlphaMode: DXGI_ALPHA_MODE_PREMULTIPLIED,
				Flags: 0,
			};

			let swapchain: IDXGISwapChain1 =
				factory.CreateSwapChainForComposition(&d3d, &desc, None)?;
			// 升级为 IDXGISwapChain3 以使用 GetCurrentBackBufferIndex（DXGI 1.3+）
			let swapchain: IDXGISwapChain3 = swapchain.cast()?;

			// ── 5. DComp ──
			let dcomp_dev: IDCompositionDevice = DCompositionCreateDevice(Some(&dxgi))?;
			let visual: IDCompositionVisual = dcomp_dev.CreateVisual()?;

			visual.SetContent(&swapchain)?;
			// 关键：target 必须保存在结构体中，否则 Drop 后 DComp 与窗口绑定解除
			let target: IDCompositionTarget = dcomp_dev.CreateTargetForHwnd(hwnd, true)?;
			target.SetRoot(&visual)?;
			dcomp_dev.Commit()?;

			// ── 6. DWrite ──
			let dwrite: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_ISOLATED)?;

			// ── 7. 构建自定义 font fallback（彩色 emoji 支持） ──
			// 需要 IDWriteFactory3（Windows 10+），如果系统不支持则跳过
			let emoji_fallback = build_emoji_fallback(&dwrite).ok();

			// ── 8. 后缓冲区 D2D bitmap 缓存（懒初始化） ──
			// FLIP_SEQUENTIAL + BufferCount=2，有 2 个缓冲区。
			// 首次 render 时通过 GetCurrentBackBufferIndex 选择对应 buffer 创建 bitmap，
			// 后续帧复用。避免每帧 CreateBitmapFromDxgiSurface。
			// 注意：不能在 new() 中预创建两个 bitmap，因为初始时 buffer 1 是 front buffer，
			// D2D 拒绝对 front buffer 创建 TARGET bitmap（E_INVALIDARG）。
			let back_buffer_bitmaps: [Option<ID2D1Bitmap1>; 2] = [None, None];

			Ok(Self {
				dcomp_device: dcomp_dev,
				dcomp_visual: visual,
				dcomp_target: target,
				d2d_context: d2d_ctx,
				dwrite_factory: dwrite,
				emoji_fallback,
				swapchain,
				screen_width: width,
				screen_height: height,
				display_area_height: height as f32, // 默认全屏
				background_enabled,
				background_opacity,
				text_format_cache: RefCell::new(HashMap::new()),
				solid_brush_cache: RefCell::new(HashMap::new()),
				back_buffer_bitmaps,
				bg_brush: None,
			})
		}
	}

	/// 从原始 RGBA 像素数据创建 D2D bitmap。
	///
	/// # 参数
	///
	/// - `data` — 预乘 alpha 的 RGBA 像素数据，长度 = width × height × 4
	/// - `width` — 图片宽度（像素）
	/// - `height` — 图片高度（像素）
	///
	/// # 返回值
	///
	/// 创建的 D2D bitmap（`ID2D1Bitmap1`），可用于 `ID2D1DeviceContext::DrawBitmap` 渲染。
	pub fn create_bitmap_from_rgba(
		&self,
		data: &[u8],
		width: u32,
		height: u32,
	) -> Result<ID2D1Bitmap1> {
		unsafe {
			let props = D2D1_BITMAP_PROPERTIES1 {
				pixelFormat: D2D1_PIXEL_FORMAT {
					format: DXGI_FORMAT_B8G8R8A8_UNORM,
					alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
				},
				dpiX: 96.0,
				dpiY: 96.0,
				bitmapOptions: D2D1_BITMAP_OPTIONS_NONE,
				colorContext: core::mem::ManuallyDrop::new(None),
			};
			let bitmap = self.d2d_context.CreateBitmap(
				D2D_SIZE_U { width, height },
				Some(data.as_ptr() as *const _),
				width * 4,
				&props,
			)?;
			Ok(bitmap)
		}
	}

	/// 测量文本渲染宽度。
	///
	/// 用于弹幕轨道分配时判断弹幕何时完全移出屏幕。
	/// 内部委托给 [`build_text_layout`](Self::build_text_layout)，复用 TextFormat 缓存。
	///
	/// # 参数
	///
	/// - `text` — 待测量的文本（UTF-16，由调用方预编码以复用资源）
	/// - `font_size` — 字体大小（像素）
	///
	/// # 返回值
	///
	/// 文本在 DWrite 布局中的宽度（像素）。失败时返回 `Err`。
	#[allow(dead_code)]
	pub fn measure_text_width(&self, text: &[u16], font_size: f32) -> Result<f32> {
		Ok(self.build_text_layout(text, font_size)?.1)
	}

	/// 构建一个可复用的 `IDWriteTextLayout`（含 font fallback 设置）。
	///
	/// 在添加弹幕时调用一次，将返回的 layout 存入 `ProcessedSegment::Text`，
	/// 渲染时每帧直接复用，避免重复创建（`IDWriteTextLayout` 是 DWrite 中最昂贵的对象）。
	///
	/// # 参数
	///
	/// - `text` — 文本内容（UTF-16）
	/// - `font_size` — 字体大小（像素）
	///
	/// # 返回值
	///
	/// 返回 `(IDWriteTextLayout, width)` — layout 和其渲染宽度（像素）。
	pub fn build_text_layout(
		&self,
		text: &[u16],
		font_size: f32,
	) -> Result<(IDWriteTextLayout, f32)> {
		unsafe {
			let fmt = self.get_or_create_text_format(font_size)?;
			let layout = self.dwrite_factory.CreateTextLayout(text, &fmt, 10000.0, 10000.0)?;
			// 一次性设置 font fallback（彩色 emoji），渲染时无需重复设置
			if let (Some(fallback), Ok(layout2)) =
				(&self.emoji_fallback, layout.cast::<IDWriteTextLayout2>())
			{
				let _ = layout2.SetFontFallback(fallback);
			}
			let mut m = DWRITE_TEXT_METRICS::default();
			layout.GetMetrics(&mut m)?;
			Ok((layout, m.width as f32))
		}
	}

	/// 设置弹幕显示区域高度。
	///
	/// 由 [`set_display_area_percent`](crate::OverlayTxt::set_display_area_percent)
	/// 驱动更新，用于背景层的绘制范围。
	pub fn set_display_area_height(&mut self, height: u32) {
		self.display_area_height = height as f32;
	}

	/// 设置是否在弹幕显示区域绘制半透明背景。
	pub fn set_background_enabled(&mut self, enabled: bool) {
		self.background_enabled = enabled;
	}

	/// 设置弹幕显示区域的背景透明度。
	pub fn set_background_opacity(&mut self, opacity: f32) {
		self.background_opacity = opacity.clamp(0.0, 1.0);
	}

	/// 渲染一帧弹幕。
	///
	/// 执行完整的渲染管线：
	/// 1. 通过 `GetCurrentBackBufferIndex` 选择预缓存的后缓冲区 bitmap
	/// 2. Clear 为全透明背景
	/// 3. 遍历弹幕列表，使用预缓存的 TextLayout 和 SolidColorBrush 绘制
	/// 4. EndDraw → Present → DComp Commit
	///
	/// # 参数
	///
	/// - `items` — 当前帧需要渲染的活跃弹幕列表
	///
	/// # 返回值
	///
	/// 渲染成功返回 `Ok(())`。如果 swapchain 丢失或 D2D 出错返回 `Err`。
	pub fn render(&mut self, items: &[DanmakuItem]) -> Result<()> {
		unsafe {
			// ── 选择当前后缓冲区对应的 bitmap（懒初始化 + 缓存复用） ──
			// 首次访问某 buffer 时通过 CreateBitmapFromDxgiSurface 创建，后续帧直接复用。
			// 不能在 new() 中预创建，因为初始 front buffer 不支持 TARGET 选项。
			let idx = self.swapchain.GetCurrentBackBufferIndex() as usize;
			if self.back_buffer_bitmaps[idx].is_none() {
				let bmp_props = D2D1_BITMAP_PROPERTIES1 {
					pixelFormat: D2D1_PIXEL_FORMAT {
						format: DXGI_FORMAT_B8G8R8A8_UNORM,
						alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
					},
					dpiX: 96.0,
					dpiY: 96.0,
					bitmapOptions: D2D1_BITMAP_OPTIONS_TARGET | D2D1_BITMAP_OPTIONS_CANNOT_DRAW,
					colorContext: core::mem::ManuallyDrop::new(None),
				};
				let surface: IDXGISurface = self.swapchain.GetBuffer(idx as u32)?;
				self.back_buffer_bitmaps[idx] = Some(
					self.d2d_context.CreateBitmapFromDxgiSurface(&surface, Some(&bmp_props as *const _))?,
				);
			}
			let d2d_bitmap = self.back_buffer_bitmaps[idx]
				.as_ref()
				.ok_or_else(|| Error::new(E_FAIL, "back buffer bitmap not initialized"))?;
			self.d2d_context.SetTarget(d2d_bitmap);
			self.d2d_context.BeginDraw();

			// Clear 为全透明（ID2D1DeviceContext 继承 ID2D1RenderTarget::Clear）
			let clear_color = D2D1_COLOR_F { r: 0.0, g: 0.0, b: 0.0, a: 0.0 };
			self.d2d_context.Clear(Some(&clear_color));

			// 在弹幕显示区域绘制半透明背景（可选）
			if self.background_enabled && self.display_area_height > 0.0 {
				let bg_color = D2D1_COLOR_F { r: 0.0, g: 0.0, b: 0.0, a: self.background_opacity };
				// 缓存背景画刷：首次创建，后续通过 SetColor 更新 opacity
				if self.bg_brush.is_none() {
					self.bg_brush =
						Some(self.d2d_context.CreateSolidColorBrush(&bg_color, None)?);
				} else {
					self.bg_brush.as_ref().unwrap().SetColor(&bg_color);
				}
				let bg_rect = D2D_RECT_F {
					left: 0.0,
					top: 0.0,
					right: self.screen_width as f32,
					bottom: self.display_area_height,
				};
				self.d2d_context.FillRectangle(&bg_rect, self.bg_brush.as_ref().unwrap());
			}

			// 绘制每条弹幕（使用预缓存的 layout 和缓存的 brush）
			let screen_w = self.screen_width as f32;
			for item in items {
				if !item.alive {
					continue;
				}
				// 可见性裁剪：完全在屏幕外则跳过绘制
				if item.x > screen_w || item.x + item.total_width < 0.0 {
					continue;
				}

				let mut cursor_x = item.x;

				for segment in &item.segments {
					match segment {
						ProcessedSegment::Text { layout, color, width, .. } => {
							let brush = match self.get_or_create_brush(color) {
								Ok(b) => b,
								Err(_) => {
									cursor_x += width;
									continue;
								}
							};
							self.d2d_context.DrawTextLayout(
								Vector2 { X: cursor_x, Y: item.y },
								layout,
								&brush,
								D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT,
							);
							cursor_x += width;
						}
						ProcessedSegment::Image { bitmap, width, height, y_offset } => {
							let dest = D2D_RECT_F {
								left: cursor_x,
								top: item.y + y_offset,
								right: cursor_x + width,
								bottom: item.y + y_offset + height,
							};
							// ID2D1DeviceContext::DrawBitmap（7 参数版本，接受 ID2D1Bitmap1）
							self.d2d_context.DrawBitmap(
								bitmap,
								Some(&dest as *const _),
								1.0,
								D2D1_INTERPOLATION_MODE_LINEAR,
								None,
								None,
							);
							cursor_x += width;
						}
					}
				}
			}

			self.d2d_context.EndDraw(None, None)?;

			if self.swapchain.Present(1, DXGI_PRESENT(0)).is_err() {
				return Err(Error::from(E_FAIL));
			}

			self.dcomp_device.Commit()?;

			Ok(())
		}
	}

	/// 获取或创建缓存的 DWrite 文本格式，避免每段每帧重复创建。
	fn get_or_create_text_format(&self, font_size: f32) -> Result<IDWriteTextFormat> {
		let key = font_size as u32;
		let mut cache = self.text_format_cache.borrow_mut();
		if let Some(fmt) = cache.get(&key) {
			return Ok(fmt.clone());
		}
		// SAFETY: create_text_format 内部调用 DWrite API，在单线程渲染循环中安全
		let fmt = unsafe { create_text_format(&self.dwrite_factory, font_size)? };
		cache.insert(key, fmt.clone());
		Ok(fmt)
	}

	/// 获取或创建缓存的实心画刷，避免每段每帧重复创建。
	///
	/// 画刷颜色来自 [u8; 4] RGBA 值，编码为单个 u64 作为缓存键。
	fn get_or_create_brush(&self, color: &[u8; 4]) -> Result<ID2D1SolidColorBrush> {
		let key = ((color[0] as u64) << 32)
			| ((color[1] as u64) << 24)
			| ((color[2] as u64) << 16)
			| ((color[3] as u64) << 8);
		let mut cache = self.solid_brush_cache.borrow_mut();
		if let Some(brush) = cache.get(&key) {
			return Ok(brush.clone());
		}
		let d2d_color = D2D1_COLOR_F {
			r: color[0] as f32 / 255.0,
			g: color[1] as f32 / 255.0,
			b: color[2] as f32 / 255.0,
			a: color[3] as f32 / 255.0,
		};
		// SAFETY: ID2D1DeviceContext 继承 ID2D1RenderTarget::CreateSolidColorBrush,
		// 在单线程渲染循环中安全调用
		let brush = unsafe { self.d2d_context.CreateSolidColorBrush(&d2d_color, None)? };
		cache.insert(key, brush.clone());
		Ok(brush)
	}
}

/// 创建 DWrite 文本格式对象。
///
/// 固定使用 Microsoft YaHei 字体，正常字重、正常样式。
///
/// # 参数
///
/// - `factory` — DWrite 工厂
/// - `font_size` — 字体大小（像素）
unsafe fn create_text_format(
	factory: &IDWriteFactory,
	font_size: f32,
) -> Result<IDWriteTextFormat> {
	// SAFETY: 调用 DWrite API 创建 TextFormat，在单线程渲染循环中安全
	unsafe {
		factory.CreateTextFormat(
			&HSTRING::from("Microsoft YaHei"),
			None,
			DWRITE_FONT_WEIGHT_NORMAL,
			DWRITE_FONT_STYLE_NORMAL,
			DWRITE_FONT_STRETCH_NORMAL,
			font_size,
			&HSTRING::from("zh-CN"),
		)
	}
}

/// 构建自定义 font fallback，让 emoji 字符优先使用彩色字体 Segoe UI Emoji。
///
/// # 原理
///
/// 默认情况下，DWrite 的 font fallback 会把 emoji 字符 fallback 到 Segoe UI Symbol
///（单色轮廓字形，即"线条感"）。通过自定义 fallback，将 emoji Unicode 范围
/// 映射到 Segoe UI Emoji（彩色 COLR/CPAL 字形），即可获得彩色 emoji。
///
/// # 返回值
///
/// 构建成功的 `IDWriteFontFallback`，或默认值（空对象，使用系统 fallback）。
///
/// # 注意
///
/// 需要 `IDWriteFactory3`，即 Windows 10+。早期 Windows 版本上此函数返回默认值。
fn build_emoji_fallback(factory: &IDWriteFactory) -> Result<IDWriteFontFallback> {
	unsafe {
		// 需要 IDWriteFactory3（Windows 10+）
		let factory3: IDWriteFactory3 = factory.cast()?;

		let builder = factory3.CreateFontFallbackBuilder()?;

		// 定义 emoji Unicode 范围
		// U+1F300–U+1F9FF: Miscellaneous Symbols, Emoticons, Supplemental Symbols
		// U+2600–U+27BF: Miscellaneous Symbols, Dingbats
		// U+FE00–U+FE0F: Variation Selectors
		// U+200D: Zero Width Joiner (ZWJ, 用于 emoji 组合)
		let emoji_ranges = [
			DWRITE_UNICODE_RANGE { first: 0x1F300, last: 0x1F9FF },
			DWRITE_UNICODE_RANGE { first: 0x2600, last: 0x27BF },
			DWRITE_UNICODE_RANGE { first: 0x200D, last: 0x200D },
			DWRITE_UNICODE_RANGE { first: 0xFE00, last: 0xFE0F },
		];

		// font family name 作为 NULL-terminated wide string
		let font_name = HSTRING::from("Segoe UI Emoji");
		let font_name_ptr = font_name.as_ptr();
		let target_family_names = [font_name_ptr];

		builder.AddMapping(
			&emoji_ranges,
			&target_family_names,
			None::<&IDWriteFontCollection>,
			PCWSTR::null(),
			PCWSTR::null(),
			1.0,
		)?;

		// 注意：不要添加系统 fallback（AddMappings），
		// 系统 fallback 中 Segoe UI Symbol 优先级高于 Segoe UI Emoji，
		// 会覆盖我们刚添加的 emoji → Segoe UI Emoji 映射。
		// 非 emoji 字符的 fallback 由 DWrite 默认机制处理。

		let fallback = builder.CreateFontFallback()?;
		Ok(fallback)
	}
}

unsafe impl Send for DcompRenderer {}
unsafe impl Sync for DcompRenderer {}
