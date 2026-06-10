
#![allow(non_snake_case)]

use std::ffi::c_void;
use std::thread;
use std::time::{Duration, Instant};

use windows::core::{GUID, HRESULT, HSTRING, IInspectable_Vtbl, IUnknown, Interface};
use windows::Win32::System::WinRT::{IMemoryBufferByteAccess, RoGetActivationFactory, RoInitialize, RO_INIT_MULTITHREADED};

use crate::mcp_server::OcrTextBlock;

const RPC_E_CHANGED_MODE: u32 = 0x80010106;

thread_local! {
    static RO_INITIALIZED: std::cell::Cell<bool> = std::cell::Cell::new(false);
}

fn ensure_ro_initialized() -> Result<(), String> {
    RO_INITIALIZED.with(|done| {
        if done.get() {
            return Ok(());
        }
        let hr = unsafe { RoInitialize(RO_INIT_MULTITHREADED) };
        match hr {
            Ok(()) => {
                done.set(true);
                Ok(())
            }
            Err(e) if e.code().0 as u32 == RPC_E_CHANGED_MODE => {

                done.set(true);
                Ok(())
            }
            Err(e) => Err(format!(
                "RoInitialize failed: 0x{:08X}",
                e.code().0 as u32
            )),
        }
    })
}

#[repr(C)]
#[derive(Default, Copy, Clone)]
struct Rect {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
}

#[repr(C)]
struct ISoftwareBitmapFactory_Vtbl {
    base__: IInspectable_Vtbl,
    Create: unsafe extern "system" fn(
        *mut c_void, i32 , i32, i32, *mut *mut c_void,
    ) -> HRESULT,
    CreateWithAlpha: unsafe extern "system" fn(
        *mut c_void, i32 , i32, i32,
        i32 , *mut *mut c_void,
    ) -> HRESULT,
}

#[repr(C)]
struct ISoftwareBitmap_Vtbl {
    base__: IInspectable_Vtbl,
    BitmapPixelFormat: unsafe extern "system" fn(*mut c_void, *mut i32) -> HRESULT,
    BitmapAlphaMode: unsafe extern "system" fn(*mut c_void, *mut i32) -> HRESULT,
    PixelWidth: unsafe extern "system" fn(*mut c_void, *mut i32) -> HRESULT,
    PixelHeight: unsafe extern "system" fn(*mut c_void, *mut i32) -> HRESULT,
    IsReadOnly: unsafe extern "system" fn(*mut c_void, *mut bool) -> HRESULT,
    SetDpiX: unsafe extern "system" fn(*mut c_void, f64) -> HRESULT,
    DpiX: unsafe extern "system" fn(*mut c_void, *mut f64) -> HRESULT,
    SetDpiY: unsafe extern "system" fn(*mut c_void, f64) -> HRESULT,
    DpiY: unsafe extern "system" fn(*mut c_void, *mut f64) -> HRESULT,
    LockBuffer: unsafe extern "system" fn(
        *mut c_void, i32 , *mut *mut c_void,
    ) -> HRESULT,
    CopyTo: unsafe extern "system" fn(*mut c_void, *mut c_void) -> HRESULT,
    CopyFromBuffer: usize,
    CopyToBuffer: usize,
    GetReadOnlyView: unsafe extern "system" fn(*mut c_void, *mut *mut c_void) -> HRESULT,
}

#[repr(C)]
struct IBitmapBuffer_Vtbl {
    base__: IInspectable_Vtbl,
    GetPlaneCount: unsafe extern "system" fn(*mut c_void, *mut i32) -> HRESULT,
    GetPlaneDescription: unsafe extern "system" fn(*mut c_void, i32, *mut c_void) -> HRESULT,
}

#[repr(C)]
struct IMemoryBuffer_Vtbl {
    base__: IInspectable_Vtbl,
    CreateReference: unsafe extern "system" fn(*mut c_void, *mut *mut c_void) -> HRESULT,
}

#[repr(C)]
struct IMemoryBufferReference_Vtbl {
    base__: IInspectable_Vtbl,
    Capacity: unsafe extern "system" fn(*mut c_void, *mut u32) -> HRESULT,
    Closed: unsafe extern "system" fn(*mut c_void, *mut c_void, *mut i64) -> HRESULT,
    RemoveClosed: unsafe extern "system" fn(*mut c_void, i64) -> HRESULT,
}

#[repr(C)]
struct IOcrEngineStatics_Vtbl {
    base__: IInspectable_Vtbl,
    MaxImageDimension: unsafe extern "system" fn(*mut c_void, *mut u32) -> HRESULT,
    AvailableRecognizerLanguages: unsafe extern "system" fn(*mut c_void, *mut *mut c_void) -> HRESULT,
    IsLanguageSupported: unsafe extern "system" fn(*mut c_void, *mut c_void, *mut bool) -> HRESULT,
    TryCreateFromLanguage: unsafe extern "system" fn(*mut c_void, *mut c_void, *mut *mut c_void) -> HRESULT,
    TryCreateFromUserProfileLanguages: unsafe extern "system" fn(*mut c_void, *mut *mut c_void) -> HRESULT,
}

#[repr(C)]
struct IOcrEngine_Vtbl {
    base__: IInspectable_Vtbl,
    RecognizeAsync: unsafe extern "system" fn(*mut c_void, *mut c_void, *mut *mut c_void) -> HRESULT,
    RecognizerLanguage: unsafe extern "system" fn(*mut c_void, *mut *mut c_void) -> HRESULT,
}

#[repr(C)]
struct IOcrResult_Vtbl {
    base__: IInspectable_Vtbl,
    Lines: unsafe extern "system" fn(*mut c_void, *mut *mut c_void) -> HRESULT,
    TextAngle: unsafe extern "system" fn(*mut c_void, *mut *mut c_void) -> HRESULT,
    Text: unsafe extern "system" fn(*mut c_void, *mut *mut c_void) -> HRESULT,
}

#[repr(C)]
struct IOcrLine_Vtbl {
    base__: IInspectable_Vtbl,
    Words: unsafe extern "system" fn(*mut c_void, *mut *mut c_void) -> HRESULT,
    Text: unsafe extern "system" fn(*mut c_void, *mut *mut c_void) -> HRESULT,
}

#[repr(C)]
struct IOcrWord_Vtbl {
    base__: IInspectable_Vtbl,
    BoundingRect: unsafe extern "system" fn(*mut c_void, *mut Rect) -> HRESULT,
    Text: unsafe extern "system" fn(*mut c_void, *mut *mut c_void) -> HRESULT,
}

#[repr(C)]
struct ILanguageFactory_Vtbl {
    base__: IInspectable_Vtbl,
    CreateLanguage: unsafe extern "system" fn(*mut c_void, *mut c_void, *mut *mut c_void) -> HRESULT,
}

#[repr(C)]
struct IAsyncInfo_Vtbl {
    base__: IInspectable_Vtbl,
    Id: unsafe extern "system" fn(*mut c_void, *mut u32) -> HRESULT,
    Status: unsafe extern "system" fn(*mut c_void, *mut i32 ) -> HRESULT,
    ErrorCode: unsafe extern "system" fn(*mut c_void, *mut HRESULT) -> HRESULT,
    Cancel: unsafe extern "system" fn(*mut c_void) -> HRESULT,
    Close: unsafe extern "system" fn(*mut c_void) -> HRESULT,
}

#[repr(C)]
struct IAsyncOperationOcrResult_Vtbl {
    base__: IInspectable_Vtbl,
    SetCompleted: unsafe extern "system" fn(*mut c_void, *mut c_void) -> HRESULT,
    Completed: unsafe extern "system" fn(*mut c_void, *mut *mut c_void) -> HRESULT,
    GetResults: unsafe extern "system" fn(*mut c_void, *mut *mut c_void) -> HRESULT,
}

#[repr(C)]
struct IVectorViewOcr_Vtbl {
    base__: IInspectable_Vtbl,
    GetAt: unsafe extern "system" fn(*mut c_void, u32, *mut *mut c_void) -> HRESULT,
    Size: unsafe extern "system" fn(*mut c_void, *mut u32) -> HRESULT,
    IndexOf: unsafe extern "system" fn(*mut c_void, *mut c_void, *mut u32, *mut bool) -> HRESULT,
    GetMany: unsafe extern "system" fn(*mut c_void, u32, u32, *mut *mut c_void, *mut u32) -> HRESULT,
}

macro_rules! decl_iface {
    ($name:ident, $vtbl:ty, $iid:expr) => {
        #[repr(transparent)]
        #[derive(Clone, PartialEq, Eq)]
        struct $name(IUnknown);

        unsafe impl Interface for $name {
            type Vtable = $vtbl;
            const IID: GUID = GUID::from_u128($iid);
        }
    };
}

decl_iface!(ISoftwareBitmapFactory,  ISoftwareBitmapFactory_Vtbl,  0xc99feb69_2d62_4d47_a6b3_4fdb6a07fdf8);
decl_iface!(ISoftwareBitmap,         ISoftwareBitmap_Vtbl,         0x689e0708_7eef_483f_963f_da938818e073);
decl_iface!(IBitmapBuffer,           IBitmapBuffer_Vtbl,           0xa53e04c4_399c_438c_b28f_a63a6b83d1a1);
decl_iface!(IMemoryBuffer,           IMemoryBuffer_Vtbl,           0xfbc4dd2a_245b_11e4_af98_689423260cf8);
decl_iface!(IMemoryBufferReference,  IMemoryBufferReference_Vtbl,  0xfbc4dd29_245b_11e4_af98_689423260cf8);
decl_iface!(IOcrEngineStatics,       IOcrEngineStatics_Vtbl,       0x5bffa85a_3384_3540_9940_699120d428a8);
decl_iface!(IOcrEngine,              IOcrEngine_Vtbl,              0x5a14bc41_5b76_3140_b680_8825562683ac);
decl_iface!(IOcrResult,              IOcrResult_Vtbl,              0x9bd235b2_175b_3d6a_92e2_388c206e2f63);
decl_iface!(IOcrLine,                IOcrLine_Vtbl,                0x0043a16f_e31f_3a24_899c_d444bd088124);
decl_iface!(IOcrWord,                IOcrWord_Vtbl,                0x3c2a477a_5cd9_3525_ba2a_23d1e0a68a1d);
decl_iface!(ILanguageFactory,        ILanguageFactory_Vtbl,        0x9b0252ac_0c27_44f8_b792_9793fb66c63e);
decl_iface!(ILanguage,               IInspectable_Vtbl,            0xea79a752_f7c2_4265_b1bd_c4dec4e4f080);
decl_iface!(IAsyncInfo,              IAsyncInfo_Vtbl,              0x00000036_0000_0000_c000_000000000046);

decl_iface!(IAsyncOperationOcrResult, IAsyncOperationOcrResult_Vtbl, 0xc7d7118e_ae36_59c0_ac76_7badee711c8b);

decl_iface!(IVectorViewOcrLine,      IVectorViewOcr_Vtbl,          0x60c76eac_8875_5ddb_a19b_65a3936279ea);

decl_iface!(IVectorViewOcrWord,      IVectorViewOcr_Vtbl,          0x805a60c7_df4f_527c_86b2_e29e439a83d2);

fn check(hr: HRESULT, ctx: &str) -> Result<(), String> {
    if hr.is_ok() {
        Ok(())
    } else {
        Err(format!("{} failed: 0x{:08X}", ctx, hr.0 as u32))
    }
}

fn factory<T: Interface>(class_name: &str) -> Result<T, String> {
    unsafe {
        RoGetActivationFactory::<T>(&HSTRING::from(class_name))
            .map_err(|e| format!("RoGetActivationFactory({}): 0x{:08X}", class_name, e.code().0 as u32))
    }
}

unsafe fn take_hstring(raw: *mut c_void) -> HSTRING {
    std::mem::transmute::<*mut c_void, HSTRING>(raw)
}

unsafe fn take<T: Interface>(raw: *mut c_void, ctx: &str) -> Result<T, String> {
    if raw.is_null() {
        Err(format!("{} returned null interface pointer", ctx))
    } else {
        Ok(T::from_raw(raw))
    }
}

fn wait_for_result(op: &IAsyncOperationOcrResult) -> Result<IOcrResult, String> {
    let info: IAsyncInfo = op.cast().map_err(|e| format!("cast to IAsyncInfo: {:?}", e))?;
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let mut status: i32 = 0;
        unsafe {
            check(
                (info.vtable().Status)(info.as_raw(), &mut status),
                "IAsyncInfo.Status",
            )?;
        }
        match status {
            0 => {

                if Instant::now() >= deadline {
                    return Err("OCR timed out after 30 s".into());
                }
                thread::sleep(Duration::from_millis(10));
            }
            1 => break,
            2 => return Err("OCR was cancelled".into()),
            3 => {

                let mut err = HRESULT(0);
                unsafe {
                    let _ = (info.vtable().ErrorCode)(info.as_raw(), &mut err);
                }
                return Err(format!("OCR failed: 0x{:08X}", err.0 as u32));
            }
            other => return Err(format!("OCR unknown async status: {}", other)),
        }
    }
    let mut result: *mut c_void = std::ptr::null_mut();
    unsafe {
        check(
            (op.vtable().GetResults)(op.as_raw(), &mut result),
            "IAsyncOperation.GetResults",
        )?;
        take(result, "IAsyncOperation.GetResults")
    }
}

fn rgba_to_premul_bgra(rgba: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(rgba.len());
    for px in rgba.chunks(4) {
        let a = px[3] as u32;
        out.push((px[2] as u32 * a / 255) as u8);
        out.push((px[1] as u32 * a / 255) as u8);
        out.push((px[0] as u32 * a / 255) as u8);
        out.push(px[3]);
    }
    out
}

fn build_software_bitmap(rgba: &[u8], width: u32, height: u32) -> Result<ISoftwareBitmap, String> {
    let sb_factory: ISoftwareBitmapFactory = factory("Windows.Graphics.Imaging.SoftwareBitmap")?;
    let mut sb_raw: *mut c_void = std::ptr::null_mut();
    unsafe {
        check(
            (sb_factory.vtable().CreateWithAlpha)(
                sb_factory.as_raw(),
                87,
                width as i32,
                height as i32,
                0,
                &mut sb_raw,
            ),
            "ISoftwareBitmapFactory.CreateWithAlpha",
        )?;
    }
    let bitmap: ISoftwareBitmap = unsafe { take(sb_raw, "CreateWithAlpha")? };

    let pixels = rgba_to_premul_bgra(rgba);

    {
        let mut buf_raw: *mut c_void = std::ptr::null_mut();
        unsafe {
            check(
                (bitmap.vtable().LockBuffer)(bitmap.as_raw(), 2 , &mut buf_raw),
                "ISoftwareBitmap.LockBuffer",
            )?;
        }
        let bitmap_buffer: IBitmapBuffer = unsafe { take(buf_raw, "LockBuffer")? };

        let mem_buffer: IMemoryBuffer = bitmap_buffer
            .cast()
            .map_err(|e| format!("cast IBitmapBuffer→IMemoryBuffer: {:?}", e))?;

        let mut ref_raw: *mut c_void = std::ptr::null_mut();
        unsafe {
            check(
                (mem_buffer.vtable().CreateReference)(mem_buffer.as_raw(), &mut ref_raw),
                "IMemoryBuffer.CreateReference",
            )?;
        }
        let mem_ref: IMemoryBufferReference = unsafe { take(ref_raw, "CreateReference")? };

        let byte_access: IMemoryBufferByteAccess = mem_ref
            .cast()
            .map_err(|e| format!("cast to IMemoryBufferByteAccess: {:?}", e))?;

        let mut ptr: *mut u8 = std::ptr::null_mut();
        let mut capacity: u32 = 0;
        unsafe {
            byte_access
                .GetBuffer(&mut ptr, &mut capacity)
                .map_err(|e| format!("IMemoryBufferByteAccess.GetBuffer: 0x{:08X}", e.code().0 as u32))?;
            if (capacity as usize) < pixels.len() {
                return Err(format!(
                    "SoftwareBitmap buffer {} too small for {} pixel bytes",
                    capacity,
                    pixels.len()
                ));
            }
            std::ptr::copy_nonoverlapping(pixels.as_ptr(), ptr, pixels.len());
        }
    }

    Ok(bitmap)
}

fn build_ocr_engine(language: Option<&str>) -> Result<IOcrEngine, String> {
    let statics: IOcrEngineStatics = factory("Windows.Media.Ocr.OcrEngine")?;

    let mut engine_raw: *mut c_void = std::ptr::null_mut();
    if let Some(lang_tag) = language {
        let lang_factory: ILanguageFactory = factory("Windows.Globalization.Language")?;
        let lang_hstr = HSTRING::from(lang_tag);
        let mut lang_raw: *mut c_void = std::ptr::null_mut();
        unsafe {
            check(
                (lang_factory.vtable().CreateLanguage)(
                    lang_factory.as_raw(),
                    std::mem::transmute_copy::<HSTRING, *mut c_void>(&lang_hstr),
                    &mut lang_raw,
                ),
                "ILanguageFactory.CreateLanguage",
            )?;
        }
        let language_obj: ILanguage = unsafe { take(lang_raw, "CreateLanguage")? };
        unsafe {
            check(
                (statics.vtable().TryCreateFromLanguage)(
                    statics.as_raw(),
                    language_obj.as_raw(),
                    &mut engine_raw,
                ),
                "OcrEngine.TryCreateFromLanguage",
            )?;
        }
    } else {
        unsafe {
            check(
                (statics.vtable().TryCreateFromUserProfileLanguages)(statics.as_raw(), &mut engine_raw),
                "OcrEngine.TryCreateFromUserProfileLanguages",
            )?;
        }
    }

    if engine_raw.is_null() {
        return Err(
            "Failed to create OcrEngine — no supported OCR language pack is installed.".into(),
        );
    }
    Ok(unsafe { IOcrEngine::from_raw(engine_raw) })
}

fn recognize(engine: &IOcrEngine, bitmap: &ISoftwareBitmap) -> Result<IOcrResult, String> {
    let mut op_raw: *mut c_void = std::ptr::null_mut();
    unsafe {
        check(
            (engine.vtable().RecognizeAsync)(engine.as_raw(), bitmap.as_raw(), &mut op_raw),
            "OcrEngine.RecognizeAsync",
        )?;
    }
    let op: IAsyncOperationOcrResult = unsafe { take(op_raw, "RecognizeAsync")? };
    wait_for_result(&op)
}

pub(crate) fn run_ocr(
    rgba: &[u8],
    width: u32,
    height: u32,
    language: Option<&str>,
) -> Result<String, String> {
    if let (6, 1, _) = nt_version::get() {
        return Err("OCR requires Windows 8 or later".into());
    }
    ensure_ro_initialized()?;

    let bitmap = build_software_bitmap(rgba, width, height)?;
    let engine = build_ocr_engine(language)?;
    let result = recognize(&engine, &bitmap)?;

    let mut text_raw: *mut c_void = std::ptr::null_mut();
    unsafe {
        check(
            (result.vtable().Text)(result.as_raw(), &mut text_raw),
            "IOcrResult.Text",
        )?;
    }
    let text = unsafe { take_hstring(text_raw) };
    Ok(text.to_string())
}

pub(crate) fn run_ocr_with_boxes(
    rgba: &[u8],
    width: u32,
    height: u32,
) -> Result<Vec<OcrTextBlock>, String> {
    if let (6, 1, _) = nt_version::get() {
        return Err("OCR requires Windows 8 or later".into());
    }
    ensure_ro_initialized()?;

    let bitmap = build_software_bitmap(rgba, width, height)?;
    let engine = build_ocr_engine(None)?;
    let result = recognize(&engine, &bitmap)?;

    let mut lines_raw: *mut c_void = std::ptr::null_mut();
    unsafe {
        check(
            (result.vtable().Lines)(result.as_raw(), &mut lines_raw),
            "IOcrResult.Lines",
        )?;
    }
    let lines: IVectorViewOcrLine = unsafe { take(lines_raw, "IOcrResult.Lines")? };

    let mut line_count: u32 = 0;
    unsafe {
        check(
            (lines.vtable().Size)(lines.as_raw(), &mut line_count),
            "IVectorView<OcrLine>.Size",
        )?;
    }

    let mut blocks = Vec::new();
    for li in 0..line_count {
        let mut line_raw: *mut c_void = std::ptr::null_mut();
        unsafe {
            check(
                (lines.vtable().GetAt)(lines.as_raw(), li, &mut line_raw),
                "IVectorView<OcrLine>.GetAt",
            )?;
        }
        let line: IOcrLine = unsafe { take(line_raw, "IVectorView<OcrLine>.GetAt")? };

        let mut words_raw: *mut c_void = std::ptr::null_mut();
        unsafe {
            check(
                (line.vtable().Words)(line.as_raw(), &mut words_raw),
                "IOcrLine.Words",
            )?;
        }
        let words: IVectorViewOcrWord = unsafe { take(words_raw, "IOcrLine.Words")? };

        let mut word_count: u32 = 0;
        unsafe {
            check(
                (words.vtable().Size)(words.as_raw(), &mut word_count),
                "IVectorView<OcrWord>.Size",
            )?;
        }

        for wi in 0..word_count {
            let mut word_raw: *mut c_void = std::ptr::null_mut();
            unsafe {
                check(
                    (words.vtable().GetAt)(words.as_raw(), wi, &mut word_raw),
                    "IVectorView<OcrWord>.GetAt",
                )?;
            }
            let word: IOcrWord = unsafe { take(word_raw, "IVectorView<OcrWord>.GetAt")? };

            let mut rect = Rect::default();
            unsafe {
                check(
                    (word.vtable().BoundingRect)(word.as_raw(), &mut rect),
                    "IOcrWord.BoundingRect",
                )?;
            }

            let mut text_raw: *mut c_void = std::ptr::null_mut();
            unsafe {
                check(
                    (word.vtable().Text)(word.as_raw(), &mut text_raw),
                    "IOcrWord.Text",
                )?;
            }
            let text = unsafe { take_hstring(text_raw).to_string() };

            blocks.push(OcrTextBlock {
                text,
                x: rect.x.round() as i32,
                y: rect.y.round() as i32,
                width: rect.width.round() as i32,
                height: rect.height.round() as i32,
            });
        }
    }

    Ok(blocks)
}
