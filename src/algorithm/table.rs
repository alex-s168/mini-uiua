//! Algorithms for tabling modifiers

use ecow::eco_vec;

use crate::{
    algorithm::{get_ops, pervade::*, zip::rows1},
    randf,
    value::Value,
    Array, ArrayValue,ImplPrimitive, Node, Ops, Primitive, Shape, SigNode, Uiua,
    UiuaResult,
};

use super::{multi_output, reduce::reduce_impl, validate_size};

pub fn table(ops: Ops, env: &mut Uiua) -> UiuaResult {
    let [f] = get_ops(ops, env)?;
    table_impl(f, env)
}

pub(crate) fn table_impl(f: SigNode, env: &mut Uiua) -> UiuaResult {
    crate::profile_function!();
    match f.sig.args {
        0 => env.exec(f),
        1 => rows1(f, env.pop(1)?, false, env),
        n => {
            let xs = env.pop(1)?;
            let ys = env.pop(2)?;
            if n == 2 && xs.rank() <= 1 && ys.rank() <= 1 {
                table_list(f, xs, ys, env)
            } else {
                if let [Node::Prim(Primitive::Mul, _), Node::Mod(Primitive::Reduce, args, _)] =
                    f.node.as_slice()
                {
                    if let [sn] = args.as_slice() {
                        if let Some((Primitive::Add, _)) = sn.node.as_flipped_primitive() {
                            match (&xs, &ys) {
                                (Value::Num(a), Value::Num(b)) => {
                                    return a.matrix_mul(b, env).map(|val| env.push(val))
                                }
                                (Value::Num(a), Value::Byte(b)) => {
                                    return a
                                        .matrix_mul(&b.convert_ref(), env)
                                        .map(|val| env.push(val))
                                }
                                (Value::Byte(a), Value::Num(b)) => {
                                    return a
                                        .convert_ref()
                                        .matrix_mul(b, env)
                                        .map(|val| env.push(val))
                                }
                                (Value::Byte(a), Value::Byte(b)) => {
                                    return a
                                        .convert_ref()
                                        .matrix_mul(&b.convert_ref(), env)
                                        .map(|val| env.push(val))
                                }
                                _ => {}
                            }
                        }
                    }
                }
                generic_table(f, xs, ys, env)
            }
        }
    }
}

fn generic_table(f: SigNode, xs: Value, ys: Value, env: &mut Uiua) -> UiuaResult {
    let sig = f.sig;
    match sig.args {
        2 => {
            let x_scalar = xs.rank() == 0;
            let y_scalar = ys.rank() == 0;
            validate_size::<f64>([sig.outputs, xs.row_count(), ys.row_count()], env)?;
            let new_shape = Shape::from([xs.row_count(), ys.row_count()]);
            let outputs = sig.outputs;
            let mut items = multi_output(outputs, Value::builder(xs.row_count() * ys.row_count()));
            let y_rows = ys.into_rows().collect::<Vec<_>>();
            env.without_fill(|env| -> UiuaResult {
                for x_row in xs.into_rows() {
                    for y_row in y_rows.iter().cloned() {
                        env.push(y_row);
                        env.push(x_row.clone());
                        env.exec(f.clone())?;
                        for i in 0..outputs {
                            items[i].add_row(env.pop("tabled function result")?, env)?;
                        }
                    }
                }
                Ok(())
            })?;
            for items in items.into_iter().rev() {
                let mut tabled = items.finish();
                let mut new_shape = new_shape.clone();
                if y_scalar {
                    new_shape.remove(1);
                }
                if x_scalar {
                    new_shape.remove(0);
                }
                new_shape.extend_from_slice(&tabled.shape()[1..]);
                *tabled.shape_mut() = new_shape;
                tabled.validate_shape();
                env.push(tabled);
            }
        }
        n => {
            let zs = env.pop(3)?;
            let mut others = Vec::with_capacity(n - 3);
            for i in 3..n {
                others.push(env.pop(i + 1)?);
            }
            validate_size::<f64>(
                [
                    sig.outputs,
                    xs.row_count(),
                    ys.row_count(),
                    zs.row_count(),
                    others.iter().map(|a| a.row_count()).product::<usize>(),
                ],
                env,
            )?;
            let mut new_shape = Shape::with_capacity(n);
            for arg in [&xs, &ys, &zs].into_iter().chain(&others) {
                new_shape.push(arg.row_count());
            }
            let outputs = sig.outputs;
            let other_rows_product = others.iter().map(|a| a.row_count()).product::<usize>();
            let mut items = multi_output(
                outputs,
                Value::builder(
                    xs.row_count() * ys.row_count() * zs.row_count() * other_rows_product,
                ),
            );
            env.without_fill(|env| -> UiuaResult {
                for x_row in xs.into_rows() {
                    for y_row in ys.rows() {
                        for z_row in zs.rows() {
                            for mut i in 0..other_rows_product {
                                for arg in others.iter().rev() {
                                    let j = i % arg.row_count();
                                    env.push(arg.row(j));
                                    i /= arg.row_count();
                                }
                                env.push(z_row.clone());
                                env.push(y_row.clone());
                                env.push(x_row.clone());
                                env.exec(f.clone())?;
                                for i in 0..outputs {
                                    items[i].add_row(env.pop("crossed function result")?, env)?;
                                }
                            }
                        }
                    }
                }
                Ok(())
            })?;
            for items in items.into_iter().rev() {
                let mut tabled = items.finish();
                let mut new_shape = new_shape.clone();
                new_shape.extend_from_slice(&tabled.shape()[1..]);
                *tabled.shape_mut() = new_shape;
                tabled.validate_shape();
                env.push(tabled);
            }
        }
    }
    Ok(())
}

pub fn table_list(f: SigNode, xs: Value, ys: Value, env: &mut Uiua) -> UiuaResult {
    crate::profile_function!();
    validate_size::<f64>([f.sig.outputs, xs.row_count(), ys.row_count()], env)?;
    match (f.node.as_flipped_primitive(), xs, ys) {
        (Some(_), Value::Num(xs), Value::Num(ys)) => {
            return generic_table(f, Value::Num(xs), Value::Num(ys), env);
        }
        (Some((prim, flipped)), Value::Byte(xs), Value::Byte(ys)) => match prim {
            Primitive::Eq => env.push(fast_table_list(xs, ys, is_eq::generic, env)?),
            Primitive::Ne => env.push(fast_table_list(xs, ys, is_ne::generic, env)?),
            Primitive::Lt if !flipped => {
                env.push(fast_table_list(xs, ys, other_is_lt::generic, env)?)
            }
            Primitive::Gt if !flipped => {
                env.push(fast_table_list(xs, ys, other_is_gt::generic, env)?)
            }
            Primitive::Le if !flipped => {
                env.push(fast_table_list(xs, ys, other_is_le::generic, env)?)
            }
            Primitive::Ge if !flipped => {
                env.push(fast_table_list(xs, ys, other_is_ge::generic, env)?)
            }
            Primitive::Add => env.push(fast_table_list(xs, ys, add::byte_byte, env)?),
            Primitive::Sub if !flipped => env.push(fast_table_list(xs, ys, sub::byte_byte, env)?),
            Primitive::Mul => env.push(fast_table_list(xs, ys, mul::byte_byte, env)?),
            Primitive::Div if !flipped => env.push(fast_table_list(xs, ys, div::byte_byte, env)?),
            Primitive::Modulus if !flipped => {
                env.push(fast_table_list(xs, ys, modulus::byte_byte, env)?)
            }
            Primitive::Atan if !flipped => env.push(fast_table_list::<f64, _>(
                xs.convert(),
                ys.convert(),
                atan2::num_num,
                env,
            )?),
            Primitive::Min => env.push(fast_table_list(xs, ys, min::byte_byte, env)?),
            Primitive::Max => env.push(fast_table_list(xs, ys, max::byte_byte, env)?),
            Primitive::Join | Primitive::Couple => {
                env.push(fast_table_list_join_or_couple(xs, ys, flipped, env)?)
            }
            _ => generic_table(f, Value::Byte(xs), Value::Byte(ys), env)?,
        },

        (Some(_), Value::Num(xs), Value::Byte(ys)) => {
            let ys = ys.convert();
            return generic_table(f, Value::Num(xs), Value::Num(ys), env);
        }
        (Some(_), Value::Byte(xs), Value::Num(ys)) => {
            let xs = xs.convert();
            return generic_table(f, Value::Num(xs), Value::Num(ys), env);
        }

        // Boxes
        (Some((Primitive::Join | Primitive::Couple, flipped)), Value::Box(xs), ys) => env.push(
            fast_table_list_join_or_couple(xs, ys.coerce_to_boxes(), flipped, env)?,
        ),
        (Some((Primitive::Join | Primitive::Couple, flipped)), xs, Value::Box(ys)) => env.push(
            fast_table_list_join_or_couple(xs.coerce_to_boxes(), ys, flipped, env)?,
        ),
        // Chars
        (
            Some((Primitive::Join | Primitive::Couple, flipped)),
            Value::Char(xs),
            Value::Char(ys),
        ) => env.push(fast_table_list_join_or_couple(xs, ys, flipped, env)?),
        (_, xs, ys) => match f.node.as_flipped_impl_primitive() {
            // Random
            Some((ImplPrimitive::ReplaceRand2, _)) => {
                let shape = [xs.row_count(), ys.row_count()];
                let mut data = eco_vec![0.0; xs.row_count() * ys.row_count()];
                for n in data.make_mut() {
                    *n = randf() as f64;
                }
                env.push(Array::new(shape, data));
            }
            _ => generic_table(f, xs, ys, env)?,
        },
    }
    Ok(())
}

fn fast_table_list<T: ArrayValue, U: ArrayValue + Default>(
    a: Array<T>,
    b: Array<T>,
    f: impl Fn(T, T) -> U,
    env: &Uiua,
) -> UiuaResult<Array<U>> {
    let elem_count = validate_size::<U>([a.data.len(), b.data.len()], env)?;
    let mut new_data = eco_vec![U::default(); elem_count];
    let data_slice = new_data.make_mut();
    let mut i = 0;
    for x in a.data {
        for y in b.data.iter().cloned() {
            data_slice[i] = f(x.clone(), y);
            i += 1;
        }
    }
    let mut new_shape = a.shape;
    new_shape.extend_from_slice(&b.shape);
    Ok(Array::new(new_shape, new_data))
}

fn fast_table_list_join_or_couple<T: ArrayValue + Default>(
    a: Array<T>,
    b: Array<T>,
    flipped: bool,
    env: &Uiua,
) -> UiuaResult<Array<T>> {
    let elem_count = validate_size::<T>([a.data.len(), b.data.len(), 2], env)?;
    let mut new_data = eco_vec![T::default(); elem_count];
    let data_slice = new_data.make_mut();
    let mut i = 0;
    if flipped {
        for x in a.data {
            for y in b.data.iter().cloned() {
                data_slice[i] = y;
                i += 1;
                data_slice[i] = x.clone();
                i += 1;
            }
        }
    } else {
        for x in a.data {
            for y in b.data.iter().cloned() {
                data_slice[i] = x.clone();
                i += 1;
                data_slice[i] = y;
                i += 1;
            }
        }
    }
    let mut new_shape = a.shape;
    new_shape.extend_from_slice(&b.shape);
    new_shape.push(2);
    Ok(Array::new(new_shape, new_data))
}

pub fn reduce_table(ops: Ops, env: &mut Uiua) -> UiuaResult {
    let [f, g] = get_ops(ops, env)?;
    let xs = env.pop(1)?;
    let ys = env.pop(2)?;
    generic_reduce_table(f, g, xs, ys, env)?;
    Ok(())
}

fn generic_reduce_table(
    f: SigNode,
    g: SigNode,
    xs: Value,
    ys: Value,
    env: &mut Uiua,
) -> UiuaResult {
    if env.value_fill().is_some() {
        env.push(ys);
        env.push(xs);
        table_impl(g, env)?;
        return reduce_impl(f, 0, env);
    }

    let mut xs = xs.into_rows();
    let mut acc = xs
        .next()
        .ok_or_else(|| env.error("Cannot reduce empty array"))?;
    let mut g_rows = Value::builder(ys.row_count());
    for y in ys.rows() {
        env.push(y);
        env.push(acc.clone());
        env.exec(g.clone())?;
        g_rows.add_row(env.pop("reduced function result")?, env)?;
    }
    acc = g_rows.finish();
    for x in xs {
        g_rows = Value::builder(ys.row_count());
        for y in ys.rows() {
            env.push(y);
            env.push(x.clone());
            env.exec(g.clone())?;
            g_rows.add_row(env.pop("reduced function result")?, env)?;
        }
        env.push(g_rows.finish());
        env.push(acc);
        env.exec(f.clone())?;
        acc = env.pop("reduced function result")?;
    }
    env.push(acc);
    Ok(())
}
