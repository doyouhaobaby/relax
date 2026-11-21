// std-core differential test (core_diff.sh): iterator adapters and ranges.

#[test]
fn a01_ranges() {
    let mut s = 0u64;
    for i in 0..10u64 {
        s += i;
    }
    for i in (0..=10u64).rev() {
        s = s * 2 + i;
    }
    for i in (3..40u64).step_by(7) {
        s += i * 1000;
    }
    println!("| ranges {}", s);
    let r = 5..9;
    println!("| contains {} {} len {}", r.contains(&5), r.contains(&9), (5..9).len());
}

#[test]
fn a02_adapters() {
    let v: Vec<u32> = (1..=20).collect();
    let evens: Vec<u32> = v.iter().copied().filter(|x| x % 2 == 0).collect();
    println!("| evens {} {}", evens.len(), evens[3]);
    let sq: u32 = v.iter().map(|x| x * x).take(5).sum();
    println!("| sq {}", sq);
    let sk: u32 = v.iter().skip(15).sum();
    println!("| skip {}", sk);
    let z: u32 = v.iter().zip(v.iter().skip(1)).map(|(a, b)| a * b).sum();
    println!("| zip {}", z);
    let ch: u32 = v.iter().take(2).chain(v.iter().skip(18)).sum();
    println!("| chain {}", ch);
    println!("| any {} all {}", v.iter().any(|x| *x == 7), v.iter().all(|x| *x < 20));
    println!("| pos {}", v.iter().position(|x| *x == 9).unwrap());
    println!("| find {}", v.iter().find(|x| **x > 13).unwrap());
    println!("| max {} min {}", v.iter().max().unwrap(), v.iter().min().unwrap());
    println!("| count {}", v.iter().filter(|x| **x % 3 == 0).count());
    let f = v.iter().fold(0u64, |a, x| a * 3 + *x as u64 % 7);
    println!("| fold {}", f);
    let fm: u32 = v.iter().filter_map(|x| if x % 4 == 0 { Some(x / 4) } else { None }).sum();
    println!("| filter_map {}", fm);
    let tw: u32 = v.iter().take_while(|x| **x < 6).sum();
    let sw: u32 = v.iter().skip_while(|x| **x < 16).sum();
    println!("| take_while {} skip_while {}", tw, sw);
    let last = v.iter().last().unwrap();
    println!("| last {} nth {}", last, v.iter().nth(4).unwrap());
    let mk = v.iter().max_by_key(|x| (**x * 7) % 10).unwrap();
    let mn = v.iter().min_by_key(|x| (**x * 7) % 10).unwrap();
    println!("| max_by_key {} min_by_key {}", mk, mn);
}

#[test]
fn a03_peekable_flat_map() {
    let v: Vec<u32> = (0..6).collect();
    let mut p = v.iter().peekable();
    let mut out = 0;
    while let Some(x) = p.next() {
        if let Some(n) = p.peek() {
            out = out * 10 + *x + **n;
        }
    }
    println!("| peek {}", out);
    let mut items = (1..3).peekable();
    let rejected = items.next_if_map(|n| Err::<u8, _>(n + 10));
    let accepted = items.next_if_map(|n| Ok(n * 2));
    println!("| next_if_map {:?} {:?}", rejected, accepted);
    let rejected = items.next_if_map_mut(|n| {
        *n += 20;
        None::<u8>
    });
    let accepted = items.next_if_map_mut(|n| Some(*n * 3));
    let end = items.next_if_map_mut(|_| Some(0));
    println!("| next_if_map_mut {:?} {:?} {:?}", rejected, accepted, end);
    // A rejected end stays peeked even when the underlying iterator can resume.
    let mut source = [None, Some(7)].into_iter();
    let mut intermittent = std::iter::from_fn(|| source.next().flatten()).peekable();
    let at_end = intermittent.next_if_map(Ok);
    let still_at_end = intermittent.next_if_map_mut(|v| Some(*v));
    println!("| peeked end {:?} {:?} {:?} {:?}", at_end, still_at_end, intermittent.next(), intermittent.next());
    let fm: u32 = v.iter().flat_map(|x| 0..*x).sum();
    println!("| flat_map {}", fm);
    let nested: Vec<Vec<u32>> = v.iter().map(|x| (0..*x).collect()).collect();
    let fl: u32 = nested.iter().flatten().sum();
    println!("| flatten {}", fl);
    let sc: Vec<u32> = v.iter().scan(0, |acc, x| { *acc += x; Some(*acc) }).collect();
    println!("| scan {} {}", sc.len(), sc[5]);
    let (a, b): (Vec<u32>, Vec<u32>) = v.iter().partition(|x| **x > 2);
    println!("| partition {} {}", a.len(), b.len());
}

#[test]
fn a04_option_result() {
    let a: Option<u32> = Some(4);
    let b: Option<u32> = None;
    println!("| map {} {}", a.map(|x| x + 1).unwrap_or(0), b.map(|x| x + 1).unwrap_or(0));
    println!("| and_then {}", a.and_then(|x| if x > 3 { Some(x * 2) } else { None }).unwrap());
    println!("| or {} {}", b.or(Some(9)).unwrap(), a.xor(b).is_some());
    println!("| filter {}", a.filter(|x| *x > 10).is_none());
    let r: Result<u32, u32> = Err(3);
    println!("| res {} {} {}", r.is_err(), r.unwrap_or(7), r.map_err(|e| e + 1).unwrap_err());
    let ok: Result<u32, u32> = Ok(5);
    println!("| ok {}", ok.map(|x| x * 3).unwrap());
    let all: Option<Vec<u32>> = (1..4).map(|x| if x > 0 { Some(x) } else { None }).collect();
    println!("| collect-opt {}", all.unwrap().len());
    let bad: Result<Vec<u32>, u32> = (1..4).map(|x| if x != 2 { Ok(x) } else { Err(x) }).collect();
    println!("| collect-res {}", bad.unwrap_err());
}
