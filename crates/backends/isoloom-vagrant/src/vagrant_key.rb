# This copy's key, in every name a provider shares between copies of the environment (its VMs,
# its networks, its libvirt domains): the one the runner gives in ISOLOOM_KEY, the same at each of
# its commands, else one made from this folder's path, so a copy run by hand has its own too.
ISOLOOM_KEY = ENV.fetch("ISOLOOM_KEY", "").then do |given|
  next given if given.match?(/\A[0-9a-z]{6}\z/)
  abort "ISOLOOM_KEY is a copy's key, 6 lowercase letters or digits, not #{given.inspect}" unless given.empty?
  hash = File.expand_path(__dir__ || Dir.pwd).bytes.reduce(0x811c9dc5) { |h, b| ((h ^ b) * 0x01000193) & 0xffffffff }
  hash.to_s(36).rjust(6, "0")[-6, 6]
end
