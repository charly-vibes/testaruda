@testitem "PkgA deep nested test" begin
    using Test
    include("../../src/PkgA.jl")
    using .PkgA
    @test PkgA.greet("Deep") == "Hello from PkgA, Deep!"
end