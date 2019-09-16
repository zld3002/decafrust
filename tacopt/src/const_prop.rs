use crate::bb::{FuncBB, NextKind, checked_simplify};
use tac::{TacKind, Operand};

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
enum Value { Unk, Const(i32), Nac }

fn meet(x: Value, y: Value) -> Value {
  match (x, y) {
    (Value::Const(x), Value::Const(y)) if x == y => Value::Const(x),
    (v, Value::Unk) | (Value::Unk, v) => v,
    _ => Value::Nac,
  }
}

fn transfer(kind: TacKind, env: &mut [Value]) {
  use TacKind::*;
  use Operand::*;
  use Value::{Const as VConst, Nac};
  match kind {
    Bin { op, dst, lr } => env[dst as usize] = match lr {
      [Const(l), Const(r)] => VConst(op.eval(l, r)),
      [Reg(l), Const(r)] => if let VConst(l) = env[l as usize] { VConst(op.eval(l, r)) } else { Nac },
      [Const(l), Reg(r)] => if let VConst(r) = env[r as usize] { VConst(op.eval(l, r)) } else { Nac },
      [Reg(l), Reg(r)] => if let (VConst(l), VConst(r)) = (env[l as usize], env[r as usize]) { VConst(op.eval(l, r)) } else { Nac },
    },
    Un { op, dst, r } => env[dst as usize] = match r[0] {
      Const(r) => VConst(op.eval(r)),
      Reg(r) => if let VConst(r) = env[r as usize] { VConst(op.eval(r)) } else { Nac }
    },
    Assign { dst, src } => env[dst as usize] = match src[0] { Const(r) => VConst(r), Reg(r) => env[r as usize] },
    Call { dst, .. } => if let Some(dst) = dst { env[dst as usize] = Nac }
    LoadInt { dst, i } => env[dst as usize] = VConst(i),
    // actually LoadStr and LoadVTbl won't give `dst` a Nac
    // but as long as the implementation is correct, `dst` can never be used in calculation, so giving them Nac is okay
    Load { dst, .. } | LoadStr { dst, .. } | LoadVTbl { dst, .. } => env[dst as usize] = Nac,
    Param { .. } | Ret { .. } | Jmp { .. } | Label { .. } | Jif { .. } | Store { .. } => {}
  }
}

pub fn work(f: &mut FuncBB) {
  let (n, each) = (f.bb.len(), f.max_reg as usize);
  let (mut flow, mut tmp) = (vec![Value::Unk; n * each], vec![Value::Unk; n * each]);
  loop {
    for (idx, b) in f.bb.iter().enumerate() {
      for next in b.next().iter().filter_map(|n| n.map(|n| n as usize)) {
        let (off, off1) = (idx * each, next * each);
        for i in 0..each {
          flow[off1 + i] = meet(flow[off1 + i], flow[off + i]);
        }
      }
    }
    for (idx, b) in f.bb.iter().enumerate() {
      let env = &mut flow[idx * each..(idx + 1) * each];
      for t in b.iter() { transfer(t.payload.borrow().kind, env); }
    }
    if flow != tmp {
      tmp.clone_from_slice(&flow);
    } else { break; }
  }
  let mut flow_changed = false;
  for (idx, b) in f.bb.iter_mut().enumerate() {
    let env = &mut flow[idx * each..(idx + 1) * each];
    for t in b.iter() {
      let mut payload = t.payload.borrow_mut();
      for r in payload.kind.rw_mut().0 {
        if let Operand::Reg(r1) = *r {
          if let Value::Const(r1) = env[r1 as usize] { *r = Operand::Const(r1); }
        }
      }
      transfer(payload.kind, env);
    }
    match &mut b.next {
      NextKind::Ret(Some(r)) => if let Operand::Reg(r1) = *r {
        if let Value::Const(r1) = env[r1 as usize] { *r = Operand::Const(r1); }
      }
      &mut NextKind::Jif { cond, z, fail, jump } => if let Value::Const(c) = env[cond as usize] {
        b.next = if (c == 0) == z { NextKind::Jmp(jump) } else { NextKind::Jmp(fail) };
        flow_changed = true;
      }
      _ => {}
    }
  }
  if flow_changed {
    f.bb = checked_simplify(std::mem::replace(&mut f.bb, Vec::new()), None).unwrap();
  }
}