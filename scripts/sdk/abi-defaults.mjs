export function renderDefaults(defaults, model) {
  const limits = model.structures.find((s) => s.name === 'am_limits_t');
  const fields = limits.fields.filter((f) => !['struct_size', 'reserved'].includes(f.name));
  if (fields.length !== Object.keys(defaults.limits).length)
    throw new Error('限额默认值字段不完整');
  for (const field of fields) {
    const value = defaults.limits[field.name];
    if (!Number.isSafeInteger(value) || value <= 0)
      throw new Error(`限额默认值非法：${field.name}`);
  }
  return [
    ...Object.entries(defaults.runtime).map(([name, value]) => {
      if (!Number.isSafeInteger(value) || value <= 0) throw new Error(`运行默认值非法：${name}`);
      return `pub const SDK_${name.toUpperCase()}: u64 = ${value};`;
    }),
    'pub fn default_limits(vision: bool) -> am_limits_t {',
    'let mut limits = am_limits_t {',
    ...fields.map((field) => `${field.name}: ${defaults.limits[field.name]},`),
    '..Default::default() };',
    'if vision { limits.max_sources=0; limits.max_subscribers_per_source=0; limits.max_decoders=0; limits.max_capture_continuous=0; limits.frame_budget_bytes=0; limits.external_hold_max_frames=0; limits.external_hold_max_bytes=limits.result_budget_bytes; }',
    'else { limits.max_models=0; limits.infer_parallelism=0; limits.realtime_max_inflight=0; limits.preprocess_budget_bytes=0; }',
    'limits }',
    'pub fn limits_within(candidate: &am_limits_t, ceiling: &am_limits_t) -> bool {',
    'candidate.reserved.iter().all(|v| *v == 0)',
    ...fields.map((field) => `&& candidate.${field.name} <= ceiling.${field.name}`),
    '}',
    '#[cfg(test)] mod tests { use super::*;',
    '#[test] fn domains_expose_only_their_limits() { let c=default_limits(false); let v=default_limits(true); assert_eq!(c.max_models,0); assert_eq!(v.max_sources,0); assert_eq!(c.shared_pool_max_total_bytes,64*1024*1024); assert_eq!(v.external_hold_max_bytes,v.result_budget_bytes); }',
    '#[test] fn tightening_allows_disable_but_not_reexpansion() { let cap=default_limits(false); let mut next=cap; next.max_sources=0; assert!(limits_within(&next,&cap)); assert!(!limits_within(&cap,&next)); next=cap; next.frame_budget_bytes+=1; assert!(!limits_within(&next,&cap)); next=cap; next.reserved[0]=1; assert!(!limits_within(&next,&cap)); }',
    '}',
  ].join('\n');
}
