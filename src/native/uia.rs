//! Screen reader support: the controls each frame shows (`ui::Node`) as a UI Automation tree,
//! so Narrator, NVDA and JAWS can name them, follow the keyboard focus and press them.
//!
//! UI Automation calls these providers on its own threads, so they only read a snapshot of
//! the controls kept behind a lock, and ask the window (a posted message) to click or focus.

// The bindings' constants keep their Windows names.
#![allow(non_upper_case_globals)]

use super::ui::{Id, Node, Role};
use std::sync::{Arc, Mutex};
use windows::core::*;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::ClientToScreen;
use windows::Win32::System::Com::SAFEARRAY;
use windows::Win32::System::Ole::{SafeArrayCreateVector, SafeArrayPutElement};
use windows::Win32::System::Variant::{VARIANT, VT_I4};
use windows::Win32::UI::Accessibility::*;
use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_APP};

/// Posted to the window: wparam is `ACT_CLICK` or `ACT_FOCUS`, lparam the control's id.
pub const WM_APP_A11Y: u32 = WM_APP + 3;
pub const ACT_CLICK: usize = 1;
pub const ACT_FOCUS: usize = 2;

/// What screen readers see: the window and the controls the last frame showed.
pub struct Tree {
    hwnd: isize,
    nodes: Vec<Node>,
    focus: Option<Id>,
    /// Pixels per DIP.
    scale: f32,
}

#[derive(Clone)]
pub struct Shared(Arc<Mutex<Tree>>);

impl Shared {
    pub fn new(hwnd: HWND) -> Self {
        Shared(Arc::new(Mutex::new(Tree { hwnd: hwnd.0 as isize, nodes: Vec::new(), focus: None, scale: 1.0 })))
    }

    /// Takes the new frame's controls. Text fields are native windows that speak for
    /// themselves, so they're left out.
    pub fn update(&self, nodes: &[Node], focus: Option<Id>, scale: f32) {
        let mut t = self.0.lock().unwrap();
        if t.nodes.len() != nodes.len() || t.nodes.iter().zip(nodes).any(|(a, b)| a != b) {
            t.nodes = nodes.iter().filter(|n| n.role != Role::Edit).cloned().collect();
        }
        t.focus = focus;
        t.scale = scale;
    }

    pub fn root(&self) -> IRawElementProviderSimple {
        Root { tree: self.clone() }.into()
    }

    /// Tells screen readers the keyboard focus moved to `id` (they then read it out).
    pub fn focus_changed(&self, id: Id) {
        unsafe {
            if !UiaClientsAreListening().as_bool() || !self.0.lock().unwrap().nodes.iter().any(|n| n.id == id) {
                return;
            }
            let el: IRawElementProviderSimple = Element { id, tree: self.clone() }.into();
            let _ = UiaRaiseAutomationEvent(&el, UIA_AutomationFocusChangedEventId);
        }
    }

    fn with<R>(&self, f: impl FnOnce(&Tree) -> R) -> R {
        f(&self.0.lock().unwrap())
    }

    fn element(&self, id: Id) -> IRawElementProviderFragment {
        Element { id, tree: self.clone() }.into()
    }

    fn post(&self, action: usize, id: Id) {
        let hwnd = self.with(|t| t.hwnd);
        unsafe {
            let _ = PostMessageW(Some(HWND(hwnd as *mut _)), WM_APP_A11Y, WPARAM(action), LPARAM(id as isize));
        }
    }
}

impl Tree {
    fn hwnd(&self) -> HWND {
        HWND(self.hwnd as *mut _)
    }

    fn node(&self, id: Id) -> Option<&Node> {
        self.nodes.iter().find(|n| n.id == id)
    }

    /// The window's client area origin on screen, in pixels.
    fn origin(&self) -> (f32, f32) {
        let mut p = POINT::default();
        unsafe {
            let _ = ClientToScreen(self.hwnd(), &mut p);
        }
        (p.x as f32, p.y as f32)
    }
}

/// UI Automation's own error codes.
fn uia_err(code: u32) -> Error {
    HRESULT(code as i32).into()
}

/// "Nothing here" for interface-returning calls (S_OK with a null pointer).
fn none<T>() -> Result<T> {
    Err(Error::empty())
}

// ---- the window ----------------------------------------------------------------------------

#[implement(IRawElementProviderSimple, IRawElementProviderFragment, IRawElementProviderFragmentRoot)]
struct Root {
    tree: Shared,
}

impl IRawElementProviderSimple_Impl for Root_Impl {
    fn ProviderOptions(&self) -> Result<ProviderOptions> {
        Ok(ProviderOptions_ServerSideProvider)
    }

    fn GetPatternProvider(&self, _pattern: UIA_PATTERN_ID) -> Result<IUnknown> {
        none()
    }

    fn GetPropertyValue(&self, _property: UIA_PROPERTY_ID) -> Result<VARIANT> {
        // The window's own provider (title, kind) answers for it.
        Ok(VARIANT::default())
    }

    fn HostRawElementProvider(&self) -> Result<IRawElementProviderSimple> {
        unsafe { UiaHostProviderFromHwnd(self.tree.with(|t| t.hwnd())) }
    }
}

impl IRawElementProviderFragment_Impl for Root_Impl {
    fn Navigate(&self, direction: NavigateDirection) -> Result<IRawElementProviderFragment> {
        let id = self.tree.with(|t| match direction {
            NavigateDirection_FirstChild => t.nodes.first().map(|n| n.id),
            NavigateDirection_LastChild => t.nodes.last().map(|n| n.id),
            _ => None,
        });
        id.map_or_else(none, |id| Ok(self.tree.element(id)))
    }

    fn GetRuntimeId(&self) -> Result<*mut SAFEARRAY> {
        Ok(std::ptr::null_mut())
    }

    fn BoundingRectangle(&self) -> Result<UiaRect> {
        // The host provider gives the window's rectangle.
        Ok(UiaRect::default())
    }

    fn GetEmbeddedFragmentRoots(&self) -> Result<*mut SAFEARRAY> {
        Ok(std::ptr::null_mut())
    }

    fn SetFocus(&self) -> Result<()> {
        Ok(())
    }

    fn FragmentRoot(&self) -> Result<IRawElementProviderFragmentRoot> {
        Ok(Root { tree: self.tree.clone() }.into())
    }
}

impl IRawElementProviderFragmentRoot_Impl for Root_Impl {
    fn ElementProviderFromPoint(&self, x: f64, y: f64) -> Result<IRawElementProviderFragment> {
        let id = self.tree.with(|t| {
            let (ox, oy) = t.origin();
            let p = ((x as f32 - ox) / t.scale, (y as f32 - oy) / t.scale);
            // Later controls are drawn on top.
            t.nodes.iter().rev().find(|n| n.visible().contains(p)).map(|n| n.id)
        });
        id.map_or_else(none, |id| Ok(self.tree.element(id)))
    }

    fn GetFocus(&self) -> Result<IRawElementProviderFragment> {
        let id = self.tree.with(|t| t.focus.filter(|f| t.node(*f).is_some()));
        id.map_or_else(none, |id| Ok(self.tree.element(id)))
    }
}

// ---- a control -----------------------------------------------------------------------------

#[implement(IRawElementProviderSimple, IRawElementProviderFragment, IInvokeProvider, IToggleProvider, ISelectionItemProvider, IValueProvider)]
struct Element {
    id: Id,
    tree: Shared,
}

impl Element {
    fn node(&self) -> Option<Node> {
        self.tree.with(|t| t.node(self.id).cloned())
    }

    fn role(&self) -> Result<Role> {
        self.node().map(|n| n.role).ok_or_else(|| uia_err(UIA_E_ELEMENTNOTAVAILABLE))
    }
}

impl IRawElementProviderSimple_Impl for Element_Impl {
    fn ProviderOptions(&self) -> Result<ProviderOptions> {
        Ok(ProviderOptions_ServerSideProvider)
    }

    fn GetPatternProvider(&self, pattern: UIA_PATTERN_ID) -> Result<IUnknown> {
        let role = self.role()?;
        let fits = match pattern {
            UIA_InvokePatternId => matches!(role, Role::Button | Role::MenuItem),
            UIA_TogglePatternId => matches!(role, Role::Toggle(_)),
            UIA_SelectionItemPatternId => matches!(role, Role::Tab(_) | Role::Choice(_)),
            UIA_ValuePatternId => role == Role::Slider,
            _ => false,
        };
        if fits {
            Ok(self.to_interface::<IUnknown>())
        } else {
            none()
        }
    }

    fn GetPropertyValue(&self, property: UIA_PROPERTY_ID) -> Result<VARIANT> {
        let Some(n) = self.node() else { return Ok(VARIANT::default()) };
        let focused = self.tree.with(|t| t.focus == Some(self.id));
        Ok(match property {
            UIA_ControlTypePropertyId => {
                let ty = match n.role {
                    Role::Button => UIA_ButtonControlTypeId,
                    Role::Toggle(_) => UIA_CheckBoxControlTypeId,
                    Role::Tab(_) => UIA_TabItemControlTypeId,
                    Role::Choice(_) => UIA_RadioButtonControlTypeId,
                    Role::Slider => UIA_SliderControlTypeId,
                    Role::MenuItem => UIA_MenuItemControlTypeId,
                    Role::Text | Role::Edit => UIA_TextControlTypeId,
                };
                VARIANT::from(ty.0)
            }
            UIA_NamePropertyId => VARIANT::from(BSTR::from(n.name.as_str())),
            UIA_AutomationIdPropertyId => VARIANT::from(BSTR::from(format!("{:x}", n.id))),
            UIA_FrameworkIdPropertyId => VARIANT::from(BSTR::from("Flax")),
            UIA_IsKeyboardFocusablePropertyId | UIA_IsEnabledPropertyId | UIA_IsControlElementPropertyId | UIA_IsContentElementPropertyId => VARIANT::from(true),
            UIA_HasKeyboardFocusPropertyId => VARIANT::from(focused),
            UIA_IsOffscreenPropertyId => VARIANT::from(n.visible().w <= 0.0),
            _ => VARIANT::default(),
        })
    }

    fn HostRawElementProvider(&self) -> Result<IRawElementProviderSimple> {
        none()
    }
}

impl IRawElementProviderFragment_Impl for Element_Impl {
    fn Navigate(&self, direction: NavigateDirection) -> Result<IRawElementProviderFragment> {
        if direction == NavigateDirection_Parent {
            return Ok(Root { tree: self.tree.clone() }.into());
        }
        let id = self.tree.with(|t| {
            let i = t.nodes.iter().position(|n| n.id == self.id)?;
            match direction {
                NavigateDirection_NextSibling => t.nodes.get(i + 1).map(|n| n.id),
                NavigateDirection_PreviousSibling => i.checked_sub(1).map(|j| t.nodes[j].id),
                _ => None,
            }
        });
        id.map_or_else(none, |id| Ok(self.tree.element(id)))
    }

    fn GetRuntimeId(&self) -> Result<*mut SAFEARRAY> {
        let parts = [UiaAppendRuntimeId as i32, self.id as i32, (self.id >> 32) as i32];
        unsafe {
            let sa = SafeArrayCreateVector(VT_I4, 0, parts.len() as u32);
            if sa.is_null() {
                return Err(E_OUTOFMEMORY.into());
            }
            for (i, v) in parts.iter().enumerate() {
                SafeArrayPutElement(sa, &(i as i32), v as *const i32 as *const _)?;
            }
            Ok(sa)
        }
    }

    fn BoundingRectangle(&self) -> Result<UiaRect> {
        Ok(self.tree.with(|t| {
            let Some(n) = t.node(self.id) else { return UiaRect::default() };
            let r = n.visible();
            let (ox, oy) = t.origin();
            let s = t.scale as f64;
            UiaRect { left: ox as f64 + r.x as f64 * s, top: oy as f64 + r.y as f64 * s, width: r.w as f64 * s, height: r.h as f64 * s }
        }))
    }

    fn GetEmbeddedFragmentRoots(&self) -> Result<*mut SAFEARRAY> {
        Ok(std::ptr::null_mut())
    }

    fn SetFocus(&self) -> Result<()> {
        self.tree.post(ACT_FOCUS, self.id);
        Ok(())
    }

    fn FragmentRoot(&self) -> Result<IRawElementProviderFragmentRoot> {
        Ok(Root { tree: self.tree.clone() }.into())
    }
}

impl IInvokeProvider_Impl for Element_Impl {
    fn Invoke(&self) -> Result<()> {
        self.role()?;
        self.tree.post(ACT_CLICK, self.id);
        Ok(())
    }
}

impl IToggleProvider_Impl for Element_Impl {
    fn Toggle(&self) -> Result<()> {
        self.role()?;
        self.tree.post(ACT_CLICK, self.id);
        Ok(())
    }

    fn ToggleState(&self) -> Result<ToggleState> {
        Ok(match self.role()? {
            Role::Toggle(true) => ToggleState_On,
            _ => ToggleState_Off,
        })
    }
}

impl ISelectionItemProvider_Impl for Element_Impl {
    fn Select(&self) -> Result<()> {
        self.role()?;
        self.tree.post(ACT_CLICK, self.id);
        Ok(())
    }

    fn AddToSelection(&self) -> Result<()> {
        Err(uia_err(UIA_E_INVALIDOPERATION))
    }

    fn RemoveFromSelection(&self) -> Result<()> {
        Err(uia_err(UIA_E_INVALIDOPERATION))
    }

    fn IsSelected(&self) -> Result<BOOL> {
        Ok(matches!(self.role()?, Role::Tab(true) | Role::Choice(true)).into())
    }

    fn SelectionContainer(&self) -> Result<IRawElementProviderSimple> {
        none()
    }
}

impl IValueProvider_Impl for Element_Impl {
    fn SetValue(&self, _val: &PCWSTR) -> Result<()> {
        Err(uia_err(UIA_E_INVALIDOPERATION))
    }

    fn Value(&self) -> Result<BSTR> {
        Ok(BSTR::from(self.node().map(|n| n.value).unwrap_or_default()))
    }

    fn IsReadOnly(&self) -> Result<BOOL> {
        Ok(true.into())
    }
}
