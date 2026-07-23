@testitem "PkgA greet works" begin
    using Test
    include("../src/PkgA.jl")
    using .PkgA
    @test PkgA.greet("World") == "Hello from PkgA, World!"
end

@testitem "PkgA add works" begin
    using Test
    include("../src/PkgA.jl")
    using .PkgA
    @test PkgA.add(2, 3) == 5
    @test PkgA.add(-1, 1) == 0
end