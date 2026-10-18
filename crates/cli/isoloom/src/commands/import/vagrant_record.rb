# Runs a Vagrantfile against a stand-in `Vagrant` module and prints, as JSON, every setting it
# makes: `isoloom import vagrant` maps them to isoloom.yml. Nothing is created or started.
# Usage: ruby vagrant_record.rb path/to/Vagrantfile
require "json"
Encoding.default_external = Encoding::UTF_8
Encoding.default_internal = Encoding::UTF_8

# One configuration object: settings (`x.y = v`), nested objects (`x.vm`), and calls with their
# arguments and the object their block configured (`x.vm.network "private_network", ip: ...`).
class Recorder
  attr_reader :settings, :children, :calls

  def initialize
    @settings = {}
    @children = {}
    @calls = []
  end

  def method_missing(name, *args, &block)
    n = name.to_s
    if n.end_with?("=")
      @settings[n.chomp("=")] = plain(args.first)
      return args.first
    end
    if args.empty? && block.nil?
      return @children[n] ||= Recorder.new
    end
    child = nil
    if block
      child = Recorder.new
      # Provider blocks take (provider, override): the override configures the machine itself.
      block.arity == 2 ? block.call(child, Recorder.new) : block.call(child)
    end
    @calls << { "name" => n, "args" => args.map { |a| plain(a) }, "block" => child }
    nil
  end

  def respond_to_missing?(*)
    true
  end

  # Values as JSON: symbols become strings; anything else unknown becomes its text.
  def plain(v)
    case v
    when Hash then v.each_with_object({}) { |(k, x), h| h[k.to_s] = plain(x) }
    when Array then v.map { |x| plain(x) }
    when Symbol then v.to_s
    when String, Integer, Float, TrueClass, FalseClass, NilClass then v
    else v.to_s
    end
  end

  def to_h
    {
      "settings" => @settings,
      "children" => @children.transform_values(&:to_h),
      "calls" => @calls.map { |c| c.merge("block" => c["block"]&.to_h) },
    }
  end
end

module Vagrant
  CONFIG = Recorder.new

  def self.configure(_version)
    yield CONFIG
  end

  def self.has_plugin?(*)
    false
  end

  def self.require_version(*); end
end

path = File.expand_path(ARGV.fetch(0))
Dir.chdir(File.dirname(path)) do
  eval(File.read(path, encoding: "UTF-8"), binding, path) # rubocop:disable Security/Eval
end
puts JSON.generate(Vagrant::CONFIG.to_h)
