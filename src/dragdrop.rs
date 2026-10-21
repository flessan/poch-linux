//! Drag-out dari Ledge via OLE: seret item ke Explorer (copy default, Shift+drag = move).

use super::*;

use std::ffi::c_void;
use std::sync::atomic::{AtomicU32, Ordering};

use windows::core::{Interface, GUID, HRESULT};
use windows::Win32::Foundation::BOOL;
use windows::Win32::System::Com::{IBindCtx, IDataObject};
use windows::Win32::System::Ole::{
    DoDragDrop, IDropSource, IDropSource_Vtbl, DROPEFFECT, DROPEFFECT_COPY, DROPEFFECT_MOVE, DROPEFFECT_NONE,
};
use windows::Win32::System::SystemServices::MODIFIERKEYS_FLAGS;
use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_SHIFT};
use windows::Win32::UI::Shell::{IShellItem, SHCreateItemFromParsingName, BHID_DataObject};

// DRAGDROP_S_* — tidak di-export oleh windows-rs 0.58, jadi konstanta manual.
const DRAGDROP_S_DROP: HRESULT = HRESULT(0x00040100);
const DRAGDROP_S_CANCEL: HRESULT = HRESULT(0x00040101);
const DRAGDROP_S_USEDEFAULTCURSORS: HRESULT = HRESULT(0x00040102);

// MK_LBUTTON / MK_RBUTTON — flag tombol mouse.
const MK_LBUTTON_FLAG: u32 = 0x0001;
const MK_RBUTTON_FLAG: u32 = 0x0002;

const IID_IUNKNOWN: GUID = GUID::from_u128(0x00000000_0000_0000_C000_000000000046);

/// Drop source sendiri: struct kompatibel COM dengan vtable pointer di depan.
#[repr(C)]
struct CustomDropSource {
    vtable: *const IDropSource_Vtbl,
    ref_count: AtomicU32,
}

unsafe extern "system" fn cs_qi(this: *mut c_void, riid: *const GUID, ppv: *mut *mut c_void) -> HRESULT {
    if ppv.is_null() {
        return E_POINTER;
    }
    let want = *riid;
    if want == <IDropSource as Interface>::IID || want == IID_IUNKNOWN {
        *ppv = this;
        cs_addref(this);
        S_OK
    } else {
        *ppv = std::ptr::null_mut();
        E_NOINTERFACE
    }
}

unsafe extern "system" fn cs_addref(this: *mut c_void) -> u32 {
    let src = this as *mut CustomDropSource;
    (*src).ref_count.fetch_add(1, Ordering::Relaxed) + 1
}

unsafe extern "system" fn cs_release(this: *mut c_void) -> u32 {
    let src = this as *mut CustomDropSource;
    let new = (*src).ref_count.fetch_sub(1, Ordering::Release) - 1;
    if new == 0 {
        std::sync::atomic::fence(Ordering::Acquire);
        drop(Box::from_raw(src));
    }
    new
}

unsafe extern "system" fn cs_query(_this: *mut c_void, escape: BOOL, keys: MODIFIERKEYS_FLAGS) -> HRESULT {
    if escape.0 != 0 {
        return DRAGDROP_S_CANCEL;
    }
    let k = keys.0;
    let lmb = (k & MK_LBUTTON_FLAG) != 0;
    let rmb = (k & MK_RBUTTON_FLAG) != 0;
    if !lmb && !rmb {
        return DRAGDROP_S_DROP;
    }
    S_OK
}

unsafe extern "system" fn cs_feedback(_this: *mut c_void, _effect: DROPEFFECT) -> HRESULT {
    DRAGDROP_S_USEDEFAULTCURSORS
}

static CS_VTABLE: IDropSource_Vtbl = IDropSource_Vtbl {
    base__: windows::core::IUnknown_Vtbl {
        QueryInterface: cs_qi,
        AddRef: cs_addref,
        Release: cs_release,
    },
    QueryContinueDrag: cs_query,
    GiveFeedback: cs_feedback,
};

/// Mulai drag OLE dari sebuah item Ledge. Return `true` jika file sudah berpindah
/// (⇒ item dihapus dari shelf); `false` jika tetap (copy / cancel).
pub(crate) unsafe fn start_shelf_drag(path: &str) -> bool {
    // Data object diambil lewat IShellItem (cara yang sama dipakai Explorer),
    // sehingga format CF_HDROP / shell IDList yang dibutuhkan target sudah lengkap.
    let h = HSTRING::from(path);
    let Ok(item) = SHCreateItemFromParsingName::<_, _, IShellItem>(&h, None::<&IBindCtx>) else {
        return false;
    };
    let Ok(dobj) = item.BindToHandler::<_, IDataObject>(None::<&IBindCtx>, &BHID_DataObject) else {
        return false;
    };

    // Bangun drop source kustom, bungkus sebagai IDropSource.
    let src_box = Box::new(CustomDropSource {
        vtable: &CS_VTABLE,
        ref_count: AtomicU32::new(1),
    });
    let src_ptr = Box::into_raw(src_box) as *mut c_void;
    let drop_src = IDropSource::from_raw(src_ptr);

    let mut effect: DROPEFFECT = DROPEFFECT_NONE;
    // Default = COPY saja (Explorer tidak akan memindah walau satu drive).
    // Shift ditekan saat mulai drag = izinkan MOVE.
    let shift = (GetAsyncKeyState(VK_SHIFT.0 as i32) as u16 & 0x8000) != 0;
    let ok = if shift { DROPEFFECT_COPY | DROPEFFECT_MOVE } else { DROPEFFECT_COPY };
    let _ = DoDragDrop(&dobj, &drop_src, ok, &mut effect);
    drop(drop_src);

    // Hapus dari Ledge hanya kalau file asli benar-benar sudah tidak ada di lokasi lama.
    !std::path::Path::new(path).exists()
}